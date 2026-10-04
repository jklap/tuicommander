//! Private file setup and one-use pairing shared by IPC and HTTP.
use super::{Config, Error, Paths};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::Write,
    sync::{Arc, LazyLock, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default, Serialize, Deserialize)]
struct SetupState {
    connected: bool,
    last_error: Option<String>,
    last_message_time: Option<u64>,
    #[serde(default)]
    updated_at: u64,
}
static STATE: LazyLock<Mutex<SetupState>> = LazyLock::new(|| Mutex::new(SetupState::default()));
pub(super) static CHANGED: tokio::sync::Notify = tokio::sync::Notify::const_new();

pub(super) fn revision(paths: &Paths) -> Vec<Option<std::time::SystemTime>> {
    ["config.json", "bot.token"]
        .iter()
        .map(|name| {
            std::fs::symlink_metadata(paths.file(name))
                .ok()
                .and_then(|meta| meta.modified().ok())
        })
        .collect()
}
pub(super) async fn changed(paths: &Paths, previous: &[Option<std::time::SystemTime>]) {
    loop {
        tokio::select! {
            _ = CHANGED.notified() => return,
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                if revision(paths) != previous { return; }
            }
        }
    }
}
pub(super) fn paths() -> Result<Paths, Error> {
    let home = dirs::home_dir().ok_or(Error::Config)?;
    Ok(Paths::new(home.join(".config/tuic-telegram")))
}

#[derive(Serialize, Deserialize)]
struct Pairing {
    code: String,
    expires_at: u64,
}
pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Change {
    Token {
        token: String,
    },
    Pair,
    AddChat {
        chat_id: String,
    },
    RemoveChat {
        chat_id: String,
    },
    Configure {
        enabled: bool,
        target_tuic_session: String,
    },
}

#[derive(Serialize)]
pub(crate) struct Snapshot {
    enabled: bool,
    token_set: bool,
    bot_alias: String,
    target_tuic_session: String,
    chats: Vec<String>,
    agents: Vec<Value>,
    connected: bool,
    last_error: Option<String>,
    last_message_time: Option<u64>,
}

fn config(paths: &Paths) -> Result<Config, Error> {
    if !paths.file("config.json").exists() {
        return Ok(Config {
            enabled: false,
            bot_alias: String::new(),
            target_tuic_session: String::new(),
        });
    }
    serde_json::from_str(&super::config::private_text(&paths.file("config.json"))?)
        .map_err(|_| Error::Config)
}
fn chats(paths: &Paths) -> Result<std::collections::BTreeSet<i64>, Error> {
    if !paths.file("allowed_chat_ids").exists() {
        return Ok(Default::default());
    }
    paths.allowlist_entries()
}
fn ensure_directory(paths: &Paths) -> Result<(), Error> {
    if !paths.directory.exists() {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&paths.directory)
            .map_err(|_| Error::PrivateFile)?;
    }
    super::config::check_directory(&paths.directory)
}
fn write(paths: &Paths, name: &str, bytes: &[u8]) -> Result<(), Error> {
    ensure_directory(paths)?;
    if std::fs::symlink_metadata(paths.file(name)).is_ok() {
        super::config::private_open(&paths.file(name), false)?;
    }
    let mut tmp =
        tempfile::NamedTempFile::new_in(&paths.directory).map_err(|_| Error::PrivateFile)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| Error::PrivateFile)?;
    }
    tmp.write_all(bytes).map_err(|_| Error::PrivateFile)?;
    tmp.as_file().sync_all().map_err(|_| Error::PrivateFile)?;
    tmp.persist(paths.file(name))
        .map_err(|_| Error::PrivateFile)?;
    Ok(())
}
fn save_config(paths: &Paths, config: &Config) -> Result<(), Error> {
    write(
        paths,
        "config.json",
        &serde_json::to_vec(config).map_err(|_| Error::Config)?,
    )
}
fn save_chats(paths: &Paths, ids: &std::collections::BTreeSet<i64>) -> Result<(), Error> {
    let text: String = ids.iter().map(|id| format!("{id}\n")).collect();
    write(paths, "allowed_chat_ids", text.as_bytes())
}
fn chat_id(text: &str) -> Result<i64, Error> {
    let id = text.parse::<i64>().map_err(|_| Error::Config)?;
    if id <= 0 || id.to_string() != text {
        return Err(Error::Config);
    }
    Ok(id)
}
pub(super) fn status(connected: bool, error: Option<Error>, message: bool) {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.connected = connected;
    state.last_error = error.map(|e| e.to_string());
    if message {
        state.last_message_time = Some(now_ms());
    }
    state.updated_at = now_ms();
    if let Ok(paths) = paths() {
        if !paths.directory.exists() {
            return;
        }
        if let Ok(bytes) = serde_json::to_vec(&*state) {
            if let Err(error) = write(&paths, "status.json", &bytes) {
                tracing::warn!(source="telegram", error=%error, "Telegram status unavailable");
            }
        }
    }
}
fn live_agent(state: &AppState, peer: &str) -> bool {
    state
        .live_pty_for_peer(peer)
        .and_then(|pty| state.session_state_with_shell(&pty))
        .is_some_and(|session| session.agent_type.is_some())
}
fn snapshot(paths: &Paths, state: &AppState) -> Result<Snapshot, Error> {
    let config = config(paths)?;
    let status: SetupState = if paths.file("status.json").exists() {
        serde_json::from_str(&super::config::private_text(&paths.file("status.json"))?)
            .map_err(|_| Error::Config)?
    } else {
        SetupState::default()
    };
    let agents = state
        .peer_agents
        .iter()
        .filter(|peer| live_agent(state, peer.key()))
        .map(|peer| json!({"id":peer.tuic_session,"name":peer.name}))
        .collect();
    Ok(Snapshot {
        enabled: config.enabled,
        token_set: super::config::private_open(&paths.file("bot.token"), false).is_ok(),
        bot_alias: config.bot_alias,
        target_tuic_session: config.target_tuic_session,
        chats: chats(paths)?.iter().map(ToString::to_string).collect(),
        agents,
        connected: status.connected && now_ms().saturating_sub(status.updated_at) <= 60_000,
        last_error: status.last_error.clone(),
        last_message_time: status.last_message_time,
    })
}

pub(crate) async fn change(state: &Arc<AppState>, change: Change) -> Result<Value, Error> {
    change_at(state, paths()?, change).await
}
async fn change_at(state: &Arc<AppState>, paths: Paths, change: Change) -> Result<Value, Error> {
    // Cross-process serialization prevents a desktop save racing daemon pairing.
    ensure_directory(&paths)?;
    let lock = super::config::private_open(&paths.file("setup.lock"), true)?;
    lock.try_lock().map_err(|_| Error::AlreadyOwned)?;
    if let Change::Token { token } = change {
        let token = zeroize::Zeroizing::new(token);
        if token.is_empty()
            || token.len() > 512
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b":_-".contains(&b))
        {
            return Err(Error::Config);
        }
        {
            let _guard = STATE.lock().map_err(|_| Error::State)?;
            write(&paths, "bot.token", token.as_bytes())?;
        }
        let bot = match super::BotApi::new(paths.clone())?
            .request("getMe", json!({}))
            .await
        {
            Ok(bot) => bot,
            Err(error) => {
                status(false, Some(error), false);
                CHANGED.notify_one();
                return Err(error);
            }
        };
        let alias = bot["username"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 64)
            .ok_or(Error::Protocol)?;
        {
            let _guard = STATE.lock().map_err(|_| Error::State)?;
            let mut config = config(&paths)?;
            config.bot_alias = alias.into();
            save_config(&paths, &config)?;
        }
        CHANGED.notify_one();
        return Ok(json!({"bot_name":alias}));
    }
    let setup = STATE.lock().map_err(|_| Error::State)?;
    match change {
        Change::Pair => {
            let code: String = uuid::Uuid::new_v4().simple().to_string()[..6].to_ascii_uppercase();
            let pairing = Pairing {
                code: code.clone(),
                expires_at: now_ms() + 600_000,
            };
            write(
                &paths,
                "pairing.json",
                &serde_json::to_vec(&pairing).map_err(|_| Error::Config)?,
            )?;
            return Ok(json!({"code":code,"expires_in_seconds":600}));
        }
        Change::AddChat { chat_id: text } => {
            let id = chat_id(&text)?;
            let mut ids = chats(&paths)?;
            ids.insert(id);
            save_chats(&paths, &ids)?;
            return Ok(json!({"ok":true}));
        }
        Change::RemoveChat { chat_id: text } => {
            let id = chat_id(&text)?;
            let mut ids = chats(&paths)?;
            ids.remove(&id);
            save_chats(&paths, &ids)?;
            return Ok(json!({"ok":true}));
        }
        Change::Configure {
            enabled,
            target_tuic_session,
        } => {
            let mut config = config(&paths)?;
            if (enabled || target_tuic_session != config.target_tuic_session)
                && (!live_agent(state, &target_tuic_session)
                    || uuid::Uuid::parse_str(&target_tuic_session).is_err())
            {
                return Err(Error::State);
            }
            if enabled
                && (config.bot_alias.is_empty()
                    || super::config::private_open(&paths.file("bot.token"), false).is_err())
            {
                return Err(Error::Config);
            }
            config.enabled = enabled;
            config.target_tuic_session = target_tuic_session;
            save_chats(&paths, &chats(&paths)?)?;
            save_config(&paths, &config)?;
        }
        Change::Token { .. } => unreachable!(),
    }
    drop(setup);
    CHANGED.notify_one();
    Ok(json!({"ok":true}))
}

/// Consume the credential before parsing ordinary mail: it never reaches an agent.
pub(super) fn pair_update(paths: &Paths, value: &Value, now: u64) -> Result<bool, Error> {
    let message = &value["message"];
    let Some(text) = message["text"].as_str() else {
        return Ok(false);
    };
    if text == "/start" {
        return Ok(true);
    }
    let Some(id) = message["chat"]["id"].as_i64().filter(|id| *id > 0) else {
        return Ok(false);
    };
    if message["chat"]["type"] != "private" {
        return Ok(false);
    }
    if !paths.file("pairing.json").exists() {
        return Ok(false);
    }
    let lock = super::config::private_open(&paths.file("setup.lock"), true)?;
    lock.try_lock().map_err(|_| Error::AlreadyOwned)?;
    pair(paths, text, id, now)
}
fn pair(paths: &Paths, text: &str, id: i64, now: u64) -> Result<bool, Error> {
    if !paths.file("pairing.json").exists() {
        return Ok(false);
    }
    let pairing: Pairing =
        serde_json::from_str(&super::config::private_text(&paths.file("pairing.json"))?)
            .map_err(|_| Error::Config)?;
    if text != pairing.code || now >= pairing.expires_at {
        return Ok(false);
    }
    let mut ids = chats(paths)?;
    ids.insert(id);
    std::fs::remove_file(paths.file("pairing.json")).map_err(|_| Error::PrivateFile)?;
    save_chats(paths, &ids)?;
    Ok(true)
}

#[tauri::command]
pub(crate) fn telegram_settings(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Snapshot, String> {
    snapshot(&paths().map_err(|e| e.to_string())?, &state).map_err(|e| e.to_string())
}
#[tauri::command]
pub(crate) async fn telegram_setup(
    state: tauri::State<'_, Arc<AppState>>,
    change: Change,
) -> Result<Value, String> {
    self::change(&state, change)
        .await
        .map_err(|e| e.to_string())
}
pub(crate) fn read(state: &AppState) -> Result<Snapshot, Error> {
    snapshot(&paths()?, state)
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
