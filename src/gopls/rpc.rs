/// Content-Length framing over tokio ChildStdin/ChildStdout (LSP wire protocol).
use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

use crate::gopls::protocol::{Notification, Request, Response};

pub struct RpcTransport {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl RpcTransport {
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        }
    }

    pub async fn call<P: Serialize, R: DeserializeOwned>(
        &mut self,
        method: &'static str,
        params: P,
    ) -> Result<Option<R>> {
        let id = self.next_id;
        self.next_id += 1;
        let req = Request {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };
        self.send_message(&req).await?;
        // Loop until we get the response matching our id; skip server-sent notifications.
        loop {
            let raw = self.recv_raw().await?;
            let envelope: serde_json::Value = serde_json::from_slice(&raw)?;
            // Server-sent notifications have no "id" field (or null id).
            if envelope.get("id").map(|v| !v.is_null()).unwrap_or(false) {
                // This is a response — deserialize as our expected type.
                let resp: Response<R> = serde_json::from_slice(&raw)?;
                if let Some(err) = resp.error {
                    bail!("gopls RPC error {}: {}", err.code, err.message);
                }
                return Ok(resp.result);
            }
            // Otherwise it's a server notification (window/logMessage, $/progress, etc.) — discard.
            tracing::debug!(
                "gopls notification: {} — {}",
                envelope.get("method").and_then(|v| v.as_str()).unwrap_or("?"),
                envelope.get("params").map(|p| p.to_string()).unwrap_or_default()
            );
        }
    }

    pub async fn notify<P: Serialize>(&mut self, method: &'static str, params: P) -> Result<()> {
        let note = Notification {
            jsonrpc: "2.0",
            method,
            params,
        };
        self.send_message(&note).await
    }

    async fn send_message<T: Serialize>(&mut self, msg: &T) -> Result<()> {
        let body = serde_json::to_vec(msg)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(&body).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Drain server-sent notifications until a stop condition is met or timeout elapses.
    /// `stop_fn` receives (method, params_json) and returns true to stop draining.
    pub async fn drain_until<F>(&mut self, timeout: Duration, mut stop_fn: F)
    where
        F: FnMut(&str, &serde_json::Value) -> bool,
    {
        let start = tokio::time::Instant::now();
        loop {
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                tracing::debug!("drain timed out after {:?}", elapsed);
                break;
            }
            let remaining = timeout - elapsed;
            let raw = match tokio::time::timeout(remaining, self.recv_raw()).await {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => { tracing::debug!("drain recv error: {}", e); break; }
                Err(_) => { tracing::debug!("drain timed out waiting for notification"); break; }
            };
            if let Ok(envelope) = serde_json::from_slice::<serde_json::Value>(&raw) {
                let method = envelope.get("method").and_then(|v| v.as_str()).unwrap_or("");
                let params = envelope.get("params").cloned().unwrap_or(serde_json::Value::Null);
                tracing::debug!("drain: {}", method);
                if stop_fn(method, &params) {
                    break;
                }
            }
        }
    }

    /// Drain until gopls says workspace packages are fully loaded.
    pub async fn drain_until_ready(&mut self, timeout: Duration) {
        self.drain_until(timeout, |method, params| {
            method == "window/showMessage"
                && params
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(|m| m.contains("Finished loading"))
                    .unwrap_or(false)
        })
        .await;
        tracing::debug!("packages loaded, ready for queries");
    }

    /// Drain until gopls publishes diagnostics for `file_uri` (meaning the file is type-checked).
    pub async fn drain_until_diagnostics(&mut self, file_uri: &str, timeout: Duration) {
        let uri = file_uri.to_string();
        self.drain_until(timeout, |method, params| {
            method == "textDocument/publishDiagnostics"
                && params
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .map(|u| u == uri)
                    .unwrap_or(false)
        })
        .await;
        tracing::debug!("diagnostics received for {}", file_uri);
    }

    /// Read one raw LSP message body (after headers) from stdout.
    async fn recv_raw(&mut self) -> Result<Vec<u8>> {
        let mut content_length: usize = 0;
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).await?;
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                break;
            }
            if let Some(rest) = line.strip_prefix("Content-Length: ") {
                content_length = rest.trim().parse().context("bad Content-Length")?;
            }
        }
        if content_length == 0 {
            bail!("no Content-Length in LSP response");
        }
        let mut buf = vec![0u8; content_length];
        self.stdout.read_exact(&mut buf).await?;
        Ok(buf)
    }
}

// ── Unit tests for framing ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::gopls::protocol::{Request, Response};

    #[test]
    fn frame_encode_decode_roundtrip() {
        let req = Request {
            jsonrpc: "2.0",
            id: 1,
            method: "initialize",
            params: serde_json::json!({"processId": null}),
        };
        let body = serde_json::to_vec(&req).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        // Verify header format
        assert!(header.starts_with("Content-Length: "));
        assert!(header.ends_with("\r\n\r\n"));

        // Verify body deserializes correctly
        let decoded: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(decoded["jsonrpc"], "2.0");
        assert_eq!(decoded["method"], "initialize");
    }

    #[test]
    fn response_error_parsed() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method not found"}}"#;
        let resp: Response<serde_json::Value> = serde_json::from_str(raw).unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}
