//! The local HTTP server that receives OAuth redirects (`http://127.0.0.1:<port>/...`).

use std::time::Duration;

use reqwest::Url;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::AuthError;

/// Binds the first free port in `ports` (0 lets the OS pick).
pub(crate) async fn bind(ports: &[u16]) -> Result<(TcpListener, u16), AuthError> {
    for &port in ports {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => {
                let port = listener.local_addr()?.port();
                return Ok((listener, port));
            }
            Err(e) => tracing::debug!("port {port} unavailable: {e}"),
        }
    }
    Err(AuthError::PortsInUse(ports.to_vec()))
}

pub(crate) fn random_state() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("OS random number generator failed");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Accepts connections until one hits `path`. Anything else (favicon
/// requests, stray probes) gets a 404 and is ignored.
pub(crate) async fn wait_for_code(
    listener: &TcpListener,
    expected_state: &str,
    path: &str,
) -> Result<String, AuthError> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let Some(target) = read_request_target(&mut stream).await else {
            continue;
        };
        let url = Url::parse(&format!("http://127.0.0.1{target}"))
            .unwrap_or_else(|_| Url::parse("http://127.0.0.1/").unwrap());
        if url.path() != path {
            respond(&mut stream, "404 Not Found", "Not found.").await;
            continue;
        }

        let param = |name: &str| {
            url.query_pairs()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.into_owned())
        };

        let result = if param("state").as_deref() != Some(expected_state) {
            Err(AuthError::StateMismatch)
        } else if let Some(error) = param("error") {
            Err(AuthError::Denied(
                param("error_description").unwrap_or(error),
            ))
        } else {
            param("code").ok_or_else(|| AuthError::Denied("no code in redirect".into()))
        };

        match &result {
            Ok(_) => {
                respond(
                    &mut stream,
                    "200 OK",
                    "Signed in to Onyx. You can close this tab.",
                )
                .await
            }
            Err(e) => {
                respond(
                    &mut stream,
                    "400 Bad Request",
                    &format!("Sign-in failed: {e}"),
                )
                .await
            }
        }
        return result;
    }
}

/// Reads the request head and returns the target of the request line
/// (`/callback?code=...`), or `None` if it isn't a readable GET.
async fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .ok()?
            .ok()?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
    }
    let head = String::from_utf8_lossy(&buf);
    let mut parts = head.lines().next()?.split_whitespace();
    (parts.next()? == "GET").then(|| parts.next().map(str::to_owned))?
}

async fn respond(stream: &mut TcpStream, status: &str, message: &str) {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Onyx</title>\
         <body style=\"font-family:system-ui;background:#16181c;color:#ecf9fb;\
         display:grid;place-items:center;height:100vh;margin:0\"><p>{message}</p>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}
