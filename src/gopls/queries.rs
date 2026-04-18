/// High-level semantic queries over the gopls JSON-RPC connection.
use anyhow::Result;
use std::path::Path;

use crate::gopls::protocol::{
    CallHierarchyCallsParams, CallHierarchyIncomingCall, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyPrepareParams, DidOpenTextDocumentParams,
    ImplementationParams, Location, ReferenceContext, ReferenceParams, TextDocumentIdentifier,
    TextDocumentItem, TextDocumentPositionParams,
};
use crate::gopls::GoplsClient;
use crate::model::Symbol;

/// Resolve path to a `file://` URI.
pub fn path_to_uri(path: &Path) -> String {
    let abs = dunce_or_plain(path);
    format!("file:///{}", abs.replace('\\', "/").trim_start_matches('/'))
}

fn dunce_or_plain(path: &Path) -> String {
    // On Windows, remove \\?\ prefix for gopls compatibility
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s.to_string()
    }
}

/// Convert a `file://` URI back to a relative forward-slash path (matching DB storage format).
///
/// DB stores paths as relative to the repo root with forward slashes (from parser.rs:
/// `path.strip_prefix(root).to_string_lossy().replace('\\', "/")`).
/// gopls returns absolute `file:///E:/...` URIs.  We strip both the scheme and the
/// root prefix so the result matches DB entries exactly.
pub fn uri_to_path(uri: &str) -> String {
    let stripped = uri
        .strip_prefix("file:///")
        .or_else(|| uri.strip_prefix("file://"))
        .unwrap_or(uri);
    // Always normalise to forward slashes (matches DB format)
    stripped.replace('\\', "/")
}

/// Strip the repo root from an absolute URI-derived path to get a DB-relative path.
/// `abs_path` is the result of `uri_to_path()` (forward slashes, may start with drive letter on Windows).
/// `root_uri` is the repo root URI used to initialise gopls (e.g. `file:///E:/h_lab/...`).
pub fn uri_to_rel_path(uri: &str, root_uri: &str) -> String {
    let abs = uri_to_path(uri);
    let root_abs = uri_to_path(root_uri);
    // root_abs may end with "/" or not
    let root_prefix = if root_abs.ends_with('/') {
        root_abs.clone()
    } else {
        format!("{}/", root_abs)
    };
    abs.strip_prefix(&root_prefix)
        .unwrap_or(&abs)
        .to_string()
}

impl GoplsClient {
    /// Build an absolute file URI for a symbol whose `file` field is repo-relative.
    /// DB stores paths as `internal/handlers/auth.go` (relative, forward slashes).
    /// gopls needs `file:///E:/repo/internal/handlers/auth.go`.
    fn sym_uri(&self, sym: &Symbol) -> String {
        // root_uri is already `file:///E:/...` with no trailing slash
        let root = self.root_uri.trim_end_matches('/');
        let rel = sym.file.trim_start_matches('/').replace('\\', "/");
        format!("{}/{}", root, rel)
    }

    /// Notify gopls that a file is open (required before semantic queries).
    /// Reads the file from disk and sends `textDocument/didOpen`, then waits
    /// briefly for gopls to finish type-checking the file.
    pub async fn open_file(&mut self, uri: &str) -> Result<()> {
        // Convert URI back to absolute disk path to read content
        let abs_path = {
            let stripped = uri
                .strip_prefix("file:///")
                .or_else(|| uri.strip_prefix("file://"))
                .unwrap_or(uri);
            #[cfg(target_os = "windows")]
            { stripped.replace('/', "\\") }
            #[cfg(not(target_os = "windows"))]
            { format!("/{}", stripped) }
        };
        let text = std::fs::read_to_string(&abs_path)
            .unwrap_or_default();
        self.transport
            .notify(
                "textDocument/didOpen",
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri: uri.to_string(),
                        language_id: "go".to_string(),
                        version: 1,
                        text,
                    },
                },
            )
            .await?;
        let t = std::time::Instant::now();
        tracing::debug!("waiting for diagnostics on {}", uri);
        self.transport
            .drain_until_diagnostics(uri, std::time::Duration::from_secs(15))
            .await;
        tracing::debug!("file type-checked ({}ms)", t.elapsed().as_millis());
        Ok(())
    }

    /// Find all callers of a symbol (call hierarchy incoming).
    pub async fn callers(&mut self, sym: &Symbol) -> Result<Vec<CallHierarchyIncomingCall>> {
        let items = self.prepare_call_hierarchy(sym).await?;
        if items.is_empty() {
            return Ok(vec![]);
        }
        let item = items.into_iter().next().unwrap();
        let t = std::time::Instant::now();
        let result: Option<Vec<CallHierarchyIncomingCall>> = self
            .transport
            .call(
                "callHierarchy/incomingCalls",
                CallHierarchyCallsParams { item },
            )
            .await?;
        let r = result.unwrap_or_default();
        tracing::debug!("LSP callHierarchy/incomingCalls -> {} results ({}ms)", r.len(), t.elapsed().as_millis());
        Ok(r)
    }

    /// Find all callees of a symbol (call hierarchy outgoing).
    pub async fn callees(&mut self, sym: &Symbol) -> Result<Vec<CallHierarchyOutgoingCall>> {
        let items = self.prepare_call_hierarchy(sym).await?;
        if items.is_empty() {
            return Ok(vec![]);
        }
        let item = items.into_iter().next().unwrap();
        let t = std::time::Instant::now();
        let result: Option<Vec<CallHierarchyOutgoingCall>> = self
            .transport
            .call(
                "callHierarchy/outgoingCalls",
                CallHierarchyCallsParams { item },
            )
            .await?;
        let r = result.unwrap_or_default();
        tracing::debug!("LSP callHierarchy/outgoingCalls -> {} results ({}ms)", r.len(), t.elapsed().as_millis());
        Ok(r)
    }

    /// Find all implementations of an interface symbol.
    pub async fn implementations(&mut self, sym: &Symbol) -> Result<Vec<Location>> {
        let uri = self.sym_uri(sym);
        self.open_file(&uri).await?;
        let params = ImplementationParams {
            text_document: TextDocumentIdentifier { uri },
            position: crate::gopls::protocol::Position {
                line: sym.line.saturating_sub(1),
                character: sym.col.saturating_sub(1),
            },
        };
        let t = std::time::Instant::now();
        let result: Option<Vec<Location>> = self
            .transport
            .call("textDocument/implementation", params)
            .await?;
        let r = result.unwrap_or_default();
        tracing::debug!("LSP textDocument/implementation -> {} results ({}ms)", r.len(), t.elapsed().as_millis());
        Ok(r)
    }

    /// Find all references to a symbol.
    pub async fn references(&mut self, sym: &Symbol) -> Result<Vec<Location>> {
        let uri = self.sym_uri(sym);
        self.open_file(&uri).await?;
        let params = ReferenceParams {
            text_document: TextDocumentIdentifier { uri },
            position: crate::gopls::protocol::Position {
                line: sym.line.saturating_sub(1),
                character: sym.col.saturating_sub(1),
            },
            context: ReferenceContext {
                include_declaration: false,
            },
        };
        let t = std::time::Instant::now();
        let result: Option<Vec<Location>> = self
            .transport
            .call("textDocument/references", params)
            .await?;
        let r = result.unwrap_or_default();
        tracing::debug!("LSP textDocument/references -> {} results ({}ms)", r.len(), t.elapsed().as_millis());
        Ok(r)
    }

    /// Definition lookup.
    pub async fn definition(&mut self, sym: &Symbol) -> Result<Vec<Location>> {
        let uri = self.sym_uri(sym);
        self.open_file(&uri).await?;
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: crate::gopls::protocol::Position {
                line: sym.line.saturating_sub(1),
                character: sym.col.saturating_sub(1),
            },
        };
        let result: Option<Vec<Location>> = self
            .transport
            .call("textDocument/definition", params)
            .await?;
        Ok(result.unwrap_or_default())
    }

    async fn prepare_call_hierarchy(&mut self, sym: &Symbol) -> Result<Vec<CallHierarchyItem>> {
        let uri = self.sym_uri(sym);
        self.open_file(&uri).await?;
        let params = CallHierarchyPrepareParams {
            text_document: TextDocumentIdentifier { uri },
            position: crate::gopls::protocol::Position {
                line: sym.line.saturating_sub(1),
                character: sym.col.saturating_sub(1),
            },
        };
        let result: Option<Vec<CallHierarchyItem>> = self
            .transport
            .call("textDocument/prepareCallHierarchy", params)
            .await?;
        Ok(result.unwrap_or_default())
    }
}
