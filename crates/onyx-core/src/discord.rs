//! Discord Rich Presence over Discord's local IPC socket (a Unix socket named
//! `discord-ipc-N` on Linux, the named pipe `\\.\pipe\discord-ipc-N` on
//! Windows). Frames are a little-endian `u32` opcode, a `u32` length and a JSON
//! body.
//!
//! [`Presence::new`] returns a handle, a future that owns the connection and
//! a receiver for [`Event`]s. While enabled the future stays connected (and
//! retries while Discord isn't running) so friends' join clicks reach us even
//! when no game is running. The handle only records what should be shown.
//!
//! Joining: an activity with a [`Party`] and a join [`Secrets`] gets Discord's
//! "Ask to Join" button and can be sent as an invite from a chat. When a
//! friend joins, their Onyx gets [`Event::Join`] with the secret.

use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, WriteHalf};
use tokio::sync::{mpsc, watch};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party: Option<Party>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secrets: Option<Secrets>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Party {
    /// 2 to 128 characters, the same for everyone in the game.
    pub id: String,
    /// Current and maximum player count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<[u32; 2]>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Secrets {
    /// Handed to whoever joins; at most 128 characters.
    pub join: String,
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

/// Something Discord told us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The user clicked Join on a friend's game; this is its join secret.
    Join(String),
    /// A friend asked to join the user's game. Answer with
    /// [`Presence::answer_join_request`].
    JoinRequest(JoinRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRequest {
    pub user_id: String,
    pub name: String,
    pub avatar_url: Option<String>,
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
    #[error("Discord closed the connection")]
    Closed,
}

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

type Frame = Result<(u32, Value), IpcError>;

/// One connection to the Discord client. A reader task turns incoming frames
/// into a channel, so waiting for events can be cancelled safely.
pub struct Ipc {
    writer: WriteHalf<Box<dyn Stream>>,
    frames: mpsc::Receiver<Frame>,
    reader: tokio::task::JoinHandle<()>,
    nonce: u64,
    /// Events that arrived while waiting for a command's reply.
    events: VecDeque<Event>,
}

impl Drop for Ipc {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl Ipc {
    /// Finds Discord's socket, does the handshake and subscribes to joins.
    pub async fn connect(client_id: &str) -> Result<Self, IpcError> {
        Self::connect_to(client_id, None).await
    }

    /// `socket` replaces the search for Discord's socket (tests use it).
    async fn connect_to(client_id: &str, socket: Option<&Path>) -> Result<Self, IpcError> {
        let stream: Box<dyn Stream> = match socket {
            #[cfg(unix)]
            Some(path) => Box::new(tokio::net::UnixStream::connect(path).await?),
            _ => open_socket().await?,
        };
        let (mut read, writer) = tokio::io::split(stream);
        let (tx, frames) = mpsc::channel(16);
        let reader = tokio::spawn(async move {
            loop {
                let frame = read_frame(&mut read).await;
                let failed = frame.is_err();
                if tx.send(frame).await.is_err() || failed {
                    return;
                }
            }
        });
        let mut ipc = Self {
            writer,
            frames,
            reader,
            nonce: 0,
            events: VecDeque::new(),
        };
        ipc.write(OP_HANDSHAKE, &json!({ "v": 1, "client_id": client_id }))
            .await?;
        // The reply is a READY dispatch, or a close frame for a bad client ID.
        let (op, body) = ipc.next_frame(Some(REPLY_TIMEOUT)).await?;
        if op == OP_CLOSE {
            return Err(IpcError::Refused(error_message(&body)));
        }
        for evt in ["ACTIVITY_JOIN", "ACTIVITY_JOIN_REQUEST"] {
            ipc.command("SUBSCRIBE", Some(evt), json!({})).await?;
        }
        Ok(ipc)
    }

    /// Shows `activity`, or clears it with `None`.
    pub async fn set_activity(&mut self, activity: Option<&Activity>) -> Result<(), IpcError> {
        let args = json!({ "pid": std::process::id(), "activity": activity });
        self.command("SET_ACTIVITY", None, args).await.map(|_| ())
    }

    /// Lets a friend who asked to join in, or turns them down.
    pub async fn answer_join_request(
        &mut self,
        user_id: &str,
        accept: bool,
    ) -> Result<(), IpcError> {
        let cmd = if accept {
            "SEND_ACTIVITY_JOIN_INVITE"
        } else {
            "CLOSE_ACTIVITY_REQUEST"
        };
        self.command(cmd, None, json!({ "user_id": user_id }))
            .await
            .map(|_| ())
    }

    /// Waits for the next event Discord sends on its own.
    pub async fn next_event(&mut self) -> Result<Event, IpcError> {
        loop {
            if let Some(event) = self.events.pop_front() {
                return Ok(event);
            }
            let (op, body) = self.next_frame(None).await?;
            self.handle_unrequested(op, &body)?;
        }
    }

    async fn command(
        &mut self,
        cmd: &str,
        evt: Option<&str>,
        args: Value,
    ) -> Result<Value, IpcError> {
        self.nonce += 1;
        let nonce = self.nonce.to_string();
        let mut body = json!({ "cmd": cmd, "args": args, "nonce": nonce });
        if let Some(evt) = evt {
            body["evt"] = evt.into();
        }
        self.write(OP_FRAME, &body).await?;
        // Reads frames until this command's reply, keeping events for later.
        loop {
            let (op, body) = self.next_frame(Some(REPLY_TIMEOUT)).await?;
            if op == OP_FRAME && body["cmd"] != "DISPATCH" {
                if body["evt"] == "ERROR" {
                    return Err(IpcError::Refused(error_message(&body["data"])));
                }
                return Ok(body);
            }
            self.handle_unrequested(op, &body)?;
        }
    }

    /// Close frames and dispatched events (pings never get here).
    fn handle_unrequested(&mut self, op: u32, body: &Value) -> Result<(), IpcError> {
        match op {
            OP_CLOSE => Err(IpcError::Refused(error_message(body))),
            _ => {
                if let Some(event) = parse_event(body) {
                    self.events.push_back(event);
                }
                Ok(())
            }
        }
    }

    async fn next_frame(&mut self, timeout: Option<Duration>) -> Frame {
        loop {
            let frame = match timeout {
                Some(t) => tokio::time::timeout(t, self.frames.recv())
                    .await
                    .map_err(|_| IpcError::Timeout)?,
                None => self.frames.recv().await,
            };
            let (op, body) = frame.ok_or(IpcError::Closed)??;
            if op == OP_PING {
                self.write(OP_PONG, &body).await?;
                continue;
            }
            return Ok((op, body));
        }
    }

    async fn write(&mut self, op: u32, body: &Value) -> Result<(), IpcError> {
        let frame = encode(op, body)?;
        self.writer.write_all(&frame).await?;
        self.writer.flush().await?;
        Ok(())
    }
}

fn parse_event(body: &Value) -> Option<Event> {
    if body["cmd"] != "DISPATCH" {
        return None;
    }
    let data = &body["data"];
    match body["evt"].as_str()? {
        "ACTIVITY_JOIN" => Some(Event::Join(data["secret"].as_str()?.to_owned())),
        "ACTIVITY_JOIN_REQUEST" => {
            let user = &data["user"];
            let user_id = user["id"].as_str()?.to_owned();
            let name = user["global_name"]
                .as_str()
                .or(user["username"].as_str())
                .unwrap_or("Someone")
                .to_owned();
            // Avatar hashes are hex, optionally with an `a_` prefix (animated).
            let avatar_url = user["avatar"]
                .as_str()
                .filter(|a| a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                .filter(|_| user_id.chars().all(|c| c.is_ascii_digit()))
                .map(|a| format!("https://cdn.discordapp.com/avatars/{user_id}/{a}.png?size=64"));
            Some(Event::JoinRequest(JoinRequest {
                user_id,
                name,
                avatar_url,
            }))
        }
        _ => None,
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

#[derive(Debug)]
enum Command {
    AnswerJoin { user_id: String, accept: bool },
}

/// Cheap handle to the presence task. Dropping every handle ends the task.
#[derive(Clone)]
pub struct Presence {
    wanted: watch::Sender<Wanted>,
    commands: mpsc::UnboundedSender<Command>,
}

impl Presence {
    /// Returns the handle, the task to spawn and the event receiver. With no
    /// `client_id` (not set at build time) the task ends right away and the
    /// handle does nothing.
    pub fn new(
        client_id: Option<&'static str>,
        enabled: bool,
    ) -> (
        Self,
        impl Future<Output = ()> + Send + 'static,
        mpsc::UnboundedReceiver<Event>,
    ) {
        let (presence, rx, commands) = Self::channel(enabled);
        let (events_tx, events) = mpsc::unbounded_channel();
        let task = async move {
            match client_id {
                Some(id) => run(id, rx, commands, events_tx, None).await,
                None => {
                    tracing::debug!(
                        "no Discord app ID was set at build time; rich presence is off"
                    );
                }
            }
        };
        (presence, task, events)
    }

    fn channel(
        enabled: bool,
    ) -> (
        Self,
        watch::Receiver<Wanted>,
        mpsc::UnboundedReceiver<Command>,
    ) {
        let (tx, rx) = watch::channel(Wanted {
            enabled,
            activity: None,
        });
        let (commands, commands_rx) = mpsc::unbounded_channel();
        (
            Self {
                wanted: tx,
                commands,
            },
            rx,
            commands_rx,
        )
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

    pub fn answer_join_request(&self, user_id: String, accept: bool) {
        let _ = self.commands.send(Command::AnswerJoin { user_id, accept });
    }
}

enum Wake {
    Changed,
    Ended,
    Command(Command),
    Event(Result<Event, IpcError>),
    Retry,
}

async fn next_event(ipc: &mut Option<Ipc>) -> Result<Event, IpcError> {
    match ipc {
        Some(conn) => conn.next_event().await,
        None => std::future::pending().await,
    }
}

async fn run(
    client_id: &str,
    mut wanted: watch::Receiver<Wanted>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::UnboundedSender<Event>,
    socket: Option<PathBuf>,
) {
    let mut ipc: Option<Ipc> = None;
    // What Discord is showing for us on this connection, once we've set it.
    let mut showing: Option<Option<Activity>> = None;
    loop {
        let want = wanted.borrow_and_update().clone();
        if !want.enabled {
            // Closing the connection clears the activity on Discord's side.
            ipc = None;
            showing = None;
        } else {
            if ipc.is_none() {
                match Ipc::connect_to(client_id, socket.as_deref()).await {
                    Ok(conn) => {
                        ipc = Some(conn);
                        showing = None;
                    }
                    Err(e) => tracing::debug!("Discord rich presence: {e}"),
                }
            }
            if let Some(conn) = &mut ipc
                && showing.as_ref() != Some(&want.activity)
            {
                match conn.set_activity(want.activity.as_ref()).await {
                    Ok(()) => showing = Some(want.activity.clone()),
                    Err(e) => {
                        tracing::warn!("Discord rich presence: {e}");
                        ipc = None;
                    }
                }
            }
        }

        let retry = want.enabled && ipc.is_none();
        let wake = tokio::select! {
            changed = wanted.changed() => if changed.is_err() { Wake::Ended } else { Wake::Changed },
            Some(cmd) = commands.recv() => Wake::Command(cmd),
            event = next_event(&mut ipc) => Wake::Event(event),
            _ = tokio::time::sleep(RETRY), if retry => Wake::Retry,
        };
        match wake {
            Wake::Ended => return,
            Wake::Changed | Wake::Retry => {}
            Wake::Command(Command::AnswerJoin { user_id, accept }) => {
                if let Some(conn) = &mut ipc
                    && let Err(e) = conn.answer_join_request(&user_id, accept).await
                {
                    tracing::warn!("Discord join request: {e}");
                    ipc = None;
                }
            }
            Wake::Event(Ok(event)) => {
                let _ = events.send(event);
            }
            Wake::Event(Err(e)) => {
                tracing::debug!("Discord IPC closed: {e}");
                ipc = None;
            }
        }
        // Events that arrived while a command waited for its reply.
        if let Some(conn) = &mut ipc {
            for event in conn.events.drain(..) {
                let _ = events.send(event);
            }
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
            party: Some(Party {
                id: "p1".into(),
                size: Some([1, 8]),
            }),
            secrets: Some(Secrets {
                join: "onyx://join/x".into(),
            }),
        };
        assert_eq!(
            serde_json::to_value(&activity).unwrap(),
            json!({
                "details": "Survival",
                "state": "Fabric 1.21.1",
                "timestamps": { "start": 1_700_000_000u64 },
                "assets": { "large_image": "grass-block", "small_image": "fabric" },
                "party": { "id": "p1", "size": [1, 8] },
                "secrets": { "join": "onyx://join/x" },
            })
        );
    }

    #[test]
    fn parses_join_events() {
        assert_eq!(
            parse_event(&json!({
                "cmd": "DISPATCH", "evt": "ACTIVITY_JOIN", "data": { "secret": "onyx://join/a" }
            })),
            Some(Event::Join("onyx://join/a".into()))
        );
        assert_eq!(
            parse_event(&json!({
                "cmd": "DISPATCH", "evt": "ACTIVITY_JOIN_REQUEST",
                "data": { "user": { "id": "42", "username": "steve", "global_name": "Steve", "avatar": "a_ff00" } }
            })),
            Some(Event::JoinRequest(JoinRequest {
                user_id: "42".into(),
                name: "Steve".into(),
                avatar_url: Some("https://cdn.discordapp.com/avatars/42/a_ff00.png?size=64".into()),
            }))
        );
        assert_eq!(parse_event(&json!({ "cmd": "SET_ACTIVITY" })), None);
    }

    #[tokio::test]
    async fn frames_round_trip() {
        let body = json!({ "cmd": "SET_ACTIVITY", "nonce": "1" });
        let frame = encode(OP_FRAME, &body).unwrap();
        assert_eq!(&frame[..4], &1u32.to_le_bytes());
        let (op, read) = read_frame(&mut frame.as_slice()).await.unwrap();
        assert_eq!((op, read), (OP_FRAME, body));
    }

    #[cfg(unix)]
    async fn expect_command(conn: &mut tokio::net::UnixStream, cmd: &str) -> Value {
        let (_, body) = read_frame(conn).await.unwrap();
        assert_eq!(body["cmd"], cmd, "{body}");
        let reply = json!({ "cmd": cmd, "evt": null, "nonce": body["nonce"] });
        conn.write_all(&encode(OP_FRAME, &reply).unwrap())
            .await
            .unwrap();
        body
    }

    /// Plays Discord's side of the socket: answers the handshake and every
    /// command, and records what it was sent.
    #[cfg(unix)]
    #[tokio::test]
    async fn presence_connects_sets_joins_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("discord-ipc-0");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();

        let (presence, rx, commands) = Presence::channel(true);
        let (events_tx, mut events) = mpsc::unbounded_channel();
        tokio::spawn(run("123", rx, commands, events_tx, Some(socket)));

        // Connects while enabled even with nothing to show.
        let (mut conn, _) = listener.accept().await.unwrap();
        let (op, hello) = read_frame(&mut conn).await.unwrap();
        assert_eq!(
            (op, hello["client_id"].as_str()),
            (OP_HANDSHAKE, Some("123"))
        );
        conn.write_all(&encode(OP_FRAME, &json!({ "cmd": "DISPATCH", "evt": "READY" })).unwrap())
            .await
            .unwrap();
        assert_eq!(
            expect_command(&mut conn, "SUBSCRIBE").await["evt"],
            "ACTIVITY_JOIN"
        );
        assert_eq!(
            expect_command(&mut conn, "SUBSCRIBE").await["evt"],
            "ACTIVITY_JOIN_REQUEST"
        );
        let clear = expect_command(&mut conn, "SET_ACTIVITY").await;
        assert!(clear["args"]["activity"].is_null());

        presence.set_activity(Some(Activity {
            details: Some("Survival".into()),
            ..Default::default()
        }));
        let set = expect_command(&mut conn, "SET_ACTIVITY").await;
        assert_eq!(set["args"]["activity"]["details"], "Survival");

        let join = json!({ "cmd": "DISPATCH", "evt": "ACTIVITY_JOIN", "data": { "secret": "s" } });
        conn.write_all(&encode(OP_FRAME, &join).unwrap())
            .await
            .unwrap();
        assert_eq!(events.recv().await, Some(Event::Join("s".into())));

        presence.answer_join_request("42".into(), true);
        let answer = expect_command(&mut conn, "SEND_ACTIVITY_JOIN_INVITE").await;
        assert_eq!(answer["args"]["user_id"], "42");

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
