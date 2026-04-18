pub mod protocol;
pub mod queries;
pub mod rpc;

use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;
use tokio::process::{Child, Command};
use tracing::info;

use crate::gopls::protocol::{InitializeParams, InitializeResult};
use crate::gopls::rpc::RpcTransport;

pub struct GoplsClient {
    pub(crate) transport: RpcTransport,
    _child: Child,
    pub server_version: Option<String>,
    pub root_uri: String,
}

impl GoplsClient {
    /// Spawn gopls and perform the LSP `initialize` handshake.
    ///
    /// Returns `Err` if `gopls` is not in PATH; callers should handle this
    /// gracefully and fall back to grep-based results.
    pub async fn new(root: &Path) -> Result<Self> {
        let root_uri: String = queries::path_to_uri(root);
        let t_spawn = std::time::Instant::now();
        info!("spawning gopls for root {}", root_uri);

        let mut child = Command::new("gopls")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("failed to spawn gopls — is it installed? run: go install golang.org/x/tools/gopls@latest")?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut transport = RpcTransport::new(stdin, stdout);

        // initialize
        let result: InitializeResult = transport
            .call(
                "initialize",
                InitializeParams {
                    process_id: Some(std::process::id()),
                    root_uri: root_uri.clone(),
                    workspace_folders: Some(vec![crate::gopls::protocol::WorkspaceFolder {
                        uri: root_uri.clone(),
                        name: "root".into(),
                    }]),
                    capabilities: serde_json::json!({
                        "textDocument": {
                            "callHierarchy": { "dynamicRegistration": false },
                            "implementation": { "dynamicRegistration": false }
                        },
                        "workspace": {
                            "workspaceFolders": true
                        }
                    }),
                    initialization_options: None,
                },
            )
            .await
            .context("gopls initialize failed")?
            .context("gopls initialize returned null")?;

        let server_version = result
            .server_info
            .as_ref()
            .and_then(|i| i.version.clone());
        tracing::debug!(
            "gopls handshake done: {:?} v{:?} ({}ms)",
            result.server_info.as_ref().map(|i| &i.name),
            server_version,
            t_spawn.elapsed().as_millis()
        );

        // initialized notification (required by LSP spec)
        transport
            .notify("initialized", serde_json::json!({}))
            .await?;

        // Open go.mod to trigger gopls package loading, then drain notifications
        // until "Finished loading packages." before returning the client.
        let sentinel_uri = format!("{}/go.mod", root_uri.trim_end_matches('/'));
        let sentinel_text = std::fs::read_to_string(
            root.join("go.mod"),
        ).unwrap_or_default();
        transport
            .notify(
                "textDocument/didOpen",
                crate::gopls::protocol::DidOpenTextDocumentParams {
                    text_document: crate::gopls::protocol::TextDocumentItem {
                        uri: sentinel_uri,
                        language_id: "go".to_string(),
                        version: 1,
                        text: sentinel_text,
                    },
                },
            )
            .await?;

        let load_wait_ms = std::env::var("GOPLS_LOAD_WAIT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(15_000u64);
        transport
            .drain_until_ready(Duration::from_millis(load_wait_ms))
            .await;
        info!("gopls workspace ready ({}ms total)", t_spawn.elapsed().as_millis());

        Ok(Self {
            transport,
            _child: child,
            server_version,
            root_uri,
        })
    }

    /// Graceful shutdown.
    pub async fn shutdown(mut self) -> Result<()> {
        let _: Option<serde_json::Value> = self
            .transport
            .call("shutdown", serde_json::json!(null))
            .await
            .unwrap_or(None);
        let _ = self
            .transport
            .notify("exit", serde_json::json!(null))
            .await;
        Ok(())
    }
}

impl Drop for GoplsClient {
    fn drop(&mut self) {
        // Best-effort: kill the child if still running.
        // Proper shutdown is done via shutdown().
    }
}

/// Check if `gopls` is available in PATH without spawning a full client.
pub fn gopls_available() -> bool {
    which::which("gopls").is_ok()
}
