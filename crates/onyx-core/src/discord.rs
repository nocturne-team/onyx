//! Discord Rich Presence over Discord's local IPC socket (a Unix socket named
//! `discord-ipc-N` on Linux, the named pipe `\\.\pipe\discord-ipc-N` on
//! Windows). Frames are a little-endian `u32` opcode, a `u32` length and a JSON
//! body.
//!
//! [`Presence::new`] returns a handle and a future that owns the connection.
//! The handle only records what should be shown; the future connects when
//! there is something to show, retries while Discord isn't running, and
//! disconnects (which clears the activity) when there is nothing.

use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::watch;

const OP_HANDSHAKE: u32 = 0;
const OP_FRAME: u32 = 1;
const OP_CLOSE: u32 = 2;
const OP_PING: u32 = 3;
const OP_PONG: u32 = 4;

/// How long to wait before trying again when Discord isn't reachable.
const RETRY: Duration = Duration::from_secs(15);
/// How long Discord gets to answer a command.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
/// Discord refuses frames bigger than this, and so do we.
const MAX_FRAME: usize = 64 * 1024;

/// What Discord shows under the user's name. Field names match Discord's
/// `SET_ACTIVITY` payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Activity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<Timestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<Assets>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Timestamps {
    /// Unix seconds; Discord counts up from here.
    pub start: u64,
}

/// Images are either the key of an art asset uploaded to the Discord app, or
/// an `https://` URL that Discord proxies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Assets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
}

/// Fits a string into Discord's text fields, which take 2 to 128 characters.
pub fn text(value: &str) -> String {
    let mut out: String = value.trim().chars().take(128).collect();
    while out.chars().count() < 2 {
        // Zero-width space: Discord trims normal whitespace before counting.
        out.push('\u{200b}');
    }
    out
}

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("Discord isn't running (no IPC socket found)")]
    NotRunning,
    #[error("Discord IPC: {0}")]
    Io(#[from] io::Error),
    #[error("Discord IPC sent invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Discord refused the request: {0}")]
    Refused(String),
    #[error("Discord didn't answer in time")]
    Timeout,
}

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

/// One connection to the Discord client.
pub struct Ipc {
    stream: Box<dyn Stream>,
    nonce: u64,
}

impl Ipc {
    /// Finds Discord's socket and does the handshake.
    pub async fn connect(client_id: &str) -> Result<Self, IpcError> {
        Self::connect_to(client_id, None).await
    }

    /// `socket` replaces the search for Discord's socket (tests use it).
    async fn connect_to(client_id: &str, socket: Option<&Path>) -> Result<Self, IpcError> {
        let stream = match socket {
            #[cfg(unix)]
            Some(path) => Box::new(tokio::net::UnixStream::connect(path).await?),
            _ => open_socket().await?,
        };
        let mut ipc = Self { stream, nonce: 0 };
        ipc.write(OP_HANDSHAKE, &json!({ "v": 1, "client_id": client_id }))
            .await?;
        // The reply is a READY dispatch, or a close frame for a bad client ID.
        ipc.reply().await?;
        Ok(ipc)
    }

    /// Shows `activity`, or clears it with `None`.
    pub async fn set_activity(&mut self, activity: Option<&Activity>) -> Result<(), IpcError> {
        self.nonce += 1;
        let body = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": activity },
            "nonce": self.nonce.to_string(),
        });
        self.write(OP_FRAME, &body).await?;
        self.reply().await.map(|_| ())
    }

    async fn write(&mut self, op: u32, body: &Value) -> Result<(), IpcError> {
        let frame = encode(op, body)?;
        self.stream.write_all(&frame).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Reads frames until a command reply, answering pings on the way.
    async fn reply(&mut self) -> Result<Value, IpcError> {
        loop {
            let (op, body) = tokio::time::timeout(REPLY_TIMEOUT, read_frame(&mut self.stream))
                .await
                .map_err(|_| IpcError::Timeout)??;
            match op {
                OP_PING => self.write(OP_PONG, &body).await?,
                OP_CLOSE => return Err(IpcError::Refused(error_message(&body))),
                _ => {
                    if body["evt"] == "ERROR" {
                        return Err(IpcError::Refused(error_message(&body["data"])));
                    }
                    return Ok(body);
                }
            }
        }
    }
}

fn error_message(body: &Value) -> String {
    body["message"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| body.to_string())
}

fn encode(op: u32, body: &Value) -> Result<Vec<u8>, IpcError> {
    let json = serde_json::to_vec(body)?;
    let mut frame = Vec::with_capacity(8 + json.len());
    frame.extend_from_slice(&op.to_le_bytes());
    frame.extend_from_slice(&(json.len() as u32).to_le_bytes());
    frame.extend_from_slice(&json);
    Ok(frame)
}

async fn read_frame(stream: &mut (impl AsyncRead + Unpin)) -> Result<(u32, Value), IpcError> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header).await?;
    let op = u32::from_le_bytes(header[..4].try_into().unwrap());
    let len = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large").into());
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;
    Ok((op, serde_json::from_slice(&body)?))
}

#[cfg(unix)]
async fn open_socket() -> Result<Box<dyn Stream>, IpcError> {
    let mut bases: Vec<PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .filter_map(|key| std::env::var_os(key).map(PathBuf::from))
        .collect();
    bases.push(PathBuf::from("/tmp"));
    // Flatpak and Snap builds of Discord (and Vesktop) put the socket in their
    // own sandbox folder under the runtime dir.
    let sandboxes = [
        "",
        "app/com.discordapp.Discord",
        "app/com.discordapp.DiscordCanary",
        "app/dev.vencord.Vesktop",
        "snap.discord",
        "snap.discord-canary",
        ".flatpak/dev.vencord.Vesktop/xdg-run",
    ];
    for base in &bases {
        for sandbox in sandboxes {
            for n in 0..10 {
                let path = base.join(sandbox).join(format!("discord-ipc-{n}"));
                if let Ok(stream) = tokio::net::UnixStream::connect(&path).await {
                    return Ok(Box::new(stream));
                }
            }
        }
    }
    Err(IpcError::NotRunning)
}

#[cfg(windows)]
async fn open_socket() -> Result<Box<dyn Stream>, IpcError> {
    use tokio::net::windows::named_pipe::ClientOptions;

    for n in 0..10 {
        if let Ok(pipe) = ClientOptions::new().open(format!(r"\\.\pipe\discord-ipc-{n}")) {
            return Ok(Box::new(pipe));
        }
    }
    Err(IpcError::NotRunning)
}

#[cfg(not(any(unix, windows)))]
async fn open_socket() -> Result<Box<dyn Stream>, IpcError> {
    Err(IpcError::NotRunning)
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Wanted {
    enabled: bool,
    activity: Option<Activity>,
}

impl Wanted {
    fn shown(&self) -> Option<Activity> {
        self.activity.clone().filter(|_| self.enabled)
    }
}

/// Cheap handle to the presence task. Dropping every handle ends the task.
#[derive(Clone)]
pub struct Presence {
    wanted: watch::Sender<Wanted>,
}

impl Presence {
    /// Returns the handle and the task to spawn. With no `client_id` (not set
    /// at build time) the task ends right away and the handle does nothing.
    pub fn new(
        client_id: Option<&'static str>,
        enabled: bool,
    ) -> (Self, impl Future<Output = ()> + Send + 'static) {
        let (presence, rx) = Self::channel(enabled);
        let task = async move {
            match client_id {
                Some(id) => run(id, rx, None).await,
                None => {
                    tracing::debug!(
                        "no Discord app ID was set at build time; rich presence is off"
                    );
                }
            }
        };
        (presence, task)
    }

    fn channel(enabled: bool) -> (Self, watch::Receiver<Wanted>) {
        let (tx, rx) = watch::channel(Wanted {
            enabled,
            activity: None,
        });
        (Self { wanted: tx }, rx)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.wanted.send_if_modified(|w| {
            let changed = w.enabled != enabled;
            w.enabled = enabled;
            changed
        });
    }

    pub fn set_activity(&self, activity: Option<Activity>) {
        self.wanted.send_if_modified(|w| {
            let changed = w.activity != activity;
            w.activity = activity;
            changed
        });
    }
}

async fn run(client_id: &str, mut wanted: watch::Receiver<Wanted>, socket: Option<PathBuf>) {
    let mut ipc: Option<Ipc> = None;
    // What Discord is showing for us right now, as far as we know.
    let mut showing: Option<Activity> = None;
    loop {
        let want = wanted.borrow_and_update().shown();
        match &want {
            // Closing the connection clears the activity on Discord's side.
            None => {
                ipc = None;
                showing = None;
            }
            Some(activity) if showing.as_ref() != Some(activity) => {
                if ipc.is_none() {
                    match Ipc::connect_to(client_id, socket.as_deref()).await {
                        Ok(conn) => ipc = Some(conn),
                        Err(e) => tracing::debug!("Discord rich presence: {e}"),
                    }
                }
                if let Some(conn) = &mut ipc {
                    match conn.set_activity(Some(activity)).await {
                        Ok(()) => showing = Some(activity.clone()),
                        Err(e) => {
                            tracing::warn!("Discord rich presence: {e}");
                            ipc = None;
                        }
                    }
                }
            }
            Some(_) => {}
        }

        let retry = want.is_some() && showing != want;
        tokio::select! {
            changed = wanted.changed() => if changed.is_err() { return },
            _ = tokio::time::sleep(RETRY), if retry => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_fits_discord_limits() {
        assert_eq!(text("  Survival  "), "Survival");
        assert_eq!(text("A").chars().count(), 2);
        assert_eq!(text("").chars().count(), 2);
        assert_eq!(text(&"x".repeat(300)).chars().count(), 128);
    }

    #[test]
    fn activity_serializes_like_discord_expects() {
        let activity = Activity {
            details: Some("Survival".into()),
            state: Some("Fabric 1.21.1".into()),
            timestamps: Some(Timestamps {
                start: 1_700_000_000,
            }),
            assets: Some(Assets {
                large_image: Some("grass-block".into()),
                small_image: Some("fabric".into()),
                ..Default::default()
            }),
        };
        assert_eq!(
            serde_json::to_value(&activity).unwrap(),
            json!({
                "details": "Survival",
                "state": "Fabric 1.21.1",
                "timestamps": { "start": 1_700_000_000u64 },
                "assets": { "large_image": "grass-block", "small_image": "fabric" },
            })
        );
    }

    #[tokio::test]
    async fn frames_round_trip() {
        let body = json!({ "cmd": "SET_ACTIVITY", "nonce": "1" });
        let frame = encode(OP_FRAME, &body).unwrap();
        assert_eq!(&frame[..4], &1u32.to_le_bytes());
        let (op, read) = read_frame(&mut frame.as_slice()).await.unwrap();
        assert_eq!((op, read), (OP_FRAME, body));
    }

    /// Plays Discord's side of the socket: answers the handshake and every
    /// command, and records what it was sent.
    #[cfg(unix)]
    #[tokio::test]
    async fn presence_connects_sets_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("discord-ipc-0");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();

        let (presence, rx) = Presence::channel(true);
        tokio::spawn(run("123", rx, Some(socket)));
        presence.set_activity(Some(Activity {
            details: Some("Survival".into()),
            ..Default::default()
        }));

        let (mut conn, _) = listener.accept().await.unwrap();
        let (op, hello) = read_frame(&mut conn).await.unwrap();
        assert_eq!(
            (op, hello["client_id"].as_str()),
            (OP_HANDSHAKE, Some("123"))
        );
        conn.write_all(&encode(OP_FRAME, &json!({ "cmd": "DISPATCH", "evt": "READY" })).unwrap())
            .await
            .unwrap();

        let (_, cmd) = read_frame(&mut conn).await.unwrap();
        assert_eq!(cmd["cmd"], "SET_ACTIVITY");
        assert_eq!(cmd["args"]["activity"]["details"], "Survival");
        conn.write_all(&encode(OP_FRAME, &json!({ "cmd": "SET_ACTIVITY", "evt": null })).unwrap())
            .await
            .unwrap();

        // Turning it off drops the connection.
        presence.set_enabled(false);
        let mut rest = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), conn.read_to_end(&mut rest))
            .await
            .expect("connection should close")
            .unwrap();
        assert!(rest.is_empty());
    }
}
