//! Security-critic tests for slices 1-2. Offline: loopback fake only, fake IDs.
use super::journal::Journal;
use super::mail::Update;
use super::{Error, Inbound, Paths, PendingMail, Poll};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PEER: &str = "11111111-2222-4333-8444-555555555555";
const TOKEN: &str = "123456:FAKE_token-abc";
const OWNER_CHAT: i64 = 1111111;

#[derive(Default)]
struct Fake {
    updates: Vec<(i64, String)>,
    requests: Vec<(i64, i64)>,
    status: Option<u16>,
}
type Shared = Arc<Mutex<Fake>>;

fn private(path: &std::path::Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

fn setup() -> (tempfile::TempDir, Paths) {
    let dir = tempfile::Builder::new()
        .prefix("tg-adv")
        .tempdir_in(tuic_test_support::test_temp_root())
        .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let paths = Paths::new(dir.path().to_path_buf());
    private(&paths.file("allowed_chat_ids"), &format!("{OWNER_CHAT}\n"));
    private(&paths.file("bot.token"), TOKEN);
    private(
        &paths.file("config.json"),
        &format!(r#"{{"enabled":true,"bot_alias":"mint","target_tuic_session":"{PEER}"}}"#),
    );
    (dir, paths)
}

/// Telegram escapes every non-ASCII character as \uXXXX in response bodies.
fn escaped(value: &Value) -> String {
    let mut out = String::new();
    for c in value.to_string().chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for u in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{u:04x}"));
            }
        }
    }
    out
}

fn message(id: i64, chat: i64, kind: &str, text: &str) -> (i64, String) {
    (
        id,
        escaped(&json!({"update_id":id,"message":{"message_id":id+1,
            "chat":{"id":chat,"type":kind},"text":text}})),
    )
}

async fn serve() -> (SocketAddr, Shared) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let shared: Shared = Shared::default();
    let state = shared.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            let (head, mut body) = loop {
                let n = socket.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break (String::new(), Vec::new());
                }
                raw.extend_from_slice(&buf[..n]);
                if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    break (
                        String::from_utf8_lossy(&raw[..at]).to_lowercase(),
                        raw[at + 4..].to_vec(),
                    );
                }
            };
            let length: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            while body.len() < length {
                let n = socket.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..n]);
            }
            let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let offset = request["offset"].as_i64().unwrap_or(0);
            let limit = request["limit"].as_i64().unwrap_or(100) as usize;
            let (status, payload) = {
                let mut fake = state.lock().unwrap();
                fake.requests.push((offset, limit as i64));
                match fake.status {
                    Some(code) => (code, format!(r#"{{"ok":false,"error_code":{code}}}"#)),
                    None => {
                        let mut pick: Vec<&str> = if offset == -1 {
                            fake.updates
                                .last()
                                .map(|u| u.1.as_str())
                                .into_iter()
                                .collect()
                        } else {
                            fake.updates
                                .iter()
                                .filter(|u| u.0 >= offset)
                                .take(limit)
                                .map(|u| u.1.as_str())
                                .collect()
                        };
                        pick.truncate(limit);
                        (
                            200,
                            format!(r#"{{"ok":true,"result":[{}]}}"#, pick.join(",")),
                        )
                    }
                }
            };
            let reply = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
            let _ = socket.write_all(reply.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });
    (address, shared)
}

async fn ready(paths: &Paths, address: SocketAddr) -> Inbound {
    let mut inbound = Inbound::loopback(paths.clone(), address).unwrap();
    assert!(matches!(inbound.poll().await, Ok(Poll::Accepted(0))));
    inbound
}

fn requests(shared: &Shared) -> usize {
    shared.lock().unwrap().requests.len()
}

// Catches: a non-allowlisted stranger (bots are publicly discoverable) queues
// enough max-length emoji messages that one 100-update response exceeds the 1 MiB
// cap; get_updates then fails with Protocol on every retry and the owner is
// locked out for good.
#[tokio::test]
async fn stranger_flood_larger_than_response_cap_does_not_wedge_polling() {
    let (_dir, paths) = setup();
    let (address, shared) = serve().await;
    let mut inbound = ready(&paths, address).await;
    let flood = "😀".repeat(4096);
    shared.lock().unwrap().updates = (20001..20031)
        .map(|id| message(id, 999, "private", &flood))
        .collect();
    let mut progressed = false;
    for _ in 0..12 {
        if inbound.poll().await.is_ok() {
            progressed = true;
            break;
        }
    }
    assert!(
        progressed,
        "polling never recovered from an oversized stranger batch"
    );
}

// Catches: capacity saturation returns Err without any backoff, so a caller
// that retries immediately re-downloads the same batch in a tight loop.
#[tokio::test]
async fn capacity_saturation_does_not_cause_an_immediate_refetch() {
    let (_dir, paths) = setup();
    let (address, shared) = serve().await;
    let mut inbound = ready(&paths, address).await;
    shared.lock().unwrap().updates = (1..=100)
        .map(|id| message(id, OWNER_CHAT, "private", "hi"))
        .collect();
    assert!(matches!(inbound.poll().await, Ok(Poll::Accepted(100))));
    shared
        .lock()
        .unwrap()
        .updates
        .push(message(101, OWNER_CHAT, "private", "hi"));
    assert!(matches!(inbound.poll().await, Err(Error::Capacity)));
    let before = requests(&shared);
    let _ = inbound.poll().await;
    assert_eq!(
        requests(&shared),
        before,
        "immediate retry hit the network again"
    );
}

// Catches: a permanent 4xx (404 for a malformed/unknown token) is neither
// latched nor backed off, so polling hammers Telegram with a dead credential.
#[tokio::test]
async fn permanent_rejection_does_not_cause_an_immediate_refetch() {
    let (_dir, paths) = setup();
    let (address, shared) = serve().await;
    let mut inbound = ready(&paths, address).await;
    shared.lock().unwrap().status = Some(404);
    assert!(matches!(inbound.poll().await, Err(Error::Rejected(404))));
    let before = requests(&shared);
    let _ = inbound.poll().await;
    assert_eq!(
        requests(&shared),
        before,
        "immediate retry hit the network again"
    );
}

// Catches: the 401 latch lives only in memory, so a restart resumes polling
// with the revoked token.
#[tokio::test]
async fn unauthorized_latch_survives_restart_without_a_request() {
    let (_dir, paths) = setup();
    let (address, shared) = serve().await;
    let mut inbound = ready(&paths, address).await;
    shared.lock().unwrap().status = Some(401);
    assert!(matches!(inbound.poll().await, Err(Error::Unauthorized)));
    drop(inbound);
    let before = requests(&shared);
    let mut again = Inbound::loopback(paths.clone(), address).unwrap();
    assert!(matches!(again.poll().await, Err(Error::Unauthorized)));
    assert_eq!(requests(&shared), before);
}

// Catches: the cursor is only advanced for accepted mail, so ignored strangers
// are redelivered forever (or the cursor skips past the last update).
#[tokio::test]
async fn next_request_offset_is_last_update_id_plus_one_including_ignored() {
    let (_dir, paths) = setup();
    let (address, shared) = serve().await;
    let mut inbound = ready(&paths, address).await;
    shared.lock().unwrap().updates = vec![
        message(5, 999, "private", "stranger"),
        message(6, OWNER_CHAT, "private", "owner"),
        message(7, OWNER_CHAT, "group", "owner id but group type"),
    ];
    assert!(matches!(inbound.poll().await, Ok(Poll::Accepted(1))));
    let _ = inbound.poll().await;
    assert_eq!(shared.lock().unwrap().requests.last().unwrap().0, 8);
}

// Catches: a second process polls the same bot because the lock is advisory
// per handle, or the lock is never released on drop.
#[tokio::test]
async fn second_owner_is_refused_until_the_first_drops() {
    let (_dir, paths) = setup();
    let (address, _shared) = serve().await;
    let first = Inbound::loopback(paths.clone(), address).unwrap();
    assert!(matches!(
        Inbound::open(paths.clone()),
        Err(Error::AlreadyOwned)
    ));
    drop(first);
    assert!(Inbound::open(paths.clone()).is_ok());
}

fn mail_update(id: i64) -> Update {
    Update {
        id,
        mail: Some(PendingMail {
            id: format!("tg:mint:{id}"),
            recipient: PEER.into(),
            content: format!(r#"{{"chat_id":"{OWNER_CHAT}"}}"#),
        }),
    }
}

fn journal(paths: &Paths) -> Journal {
    let mut journal = Journal::open(paths, "mint", PEER).unwrap();
    journal.begin_bootstrap().unwrap();
    journal.finish_bootstrap(0).unwrap();
    journal
}

// Catches: a capacity failure commits the cursor or part of the batch, losing
// or half-storing phone messages (offset vs mail atomicity).
#[test]
fn capacity_failure_rolls_back_cursor_and_rows() {
    let (_dir, paths) = setup();
    let mut journal = journal(&paths);
    let batch: Vec<Update> = (1..=101).map(mail_update).collect();
    assert_eq!(journal.accept(&batch).err(), Some(Error::Capacity));
    assert_eq!(journal.state().unwrap().1, 0);
    assert!(journal.pending().unwrap().is_empty());
}

// Catches: after consumption the tombstone is lost and a redelivery after a
// crash (server re-sends from the old offset) enqueues the message again.
#[test]
fn consumed_mail_is_not_resurrected_by_redelivery() {
    let (_dir, paths) = setup();
    let mut journal = journal(&paths);
    let batch = vec![mail_update(1), mail_update(2)];
    assert_eq!(journal.accept(&batch).unwrap(), 2);
    journal.consume("tg:mint:1", PEER).unwrap();
    assert_eq!(journal.accept(&batch).unwrap(), 0);
    let ids: Vec<String> = journal
        .pending()
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(ids, vec!["tg:mint:2".to_string()]);
}

// Catches: consume() honors a mail ID addressed to another peer, letting one
// peer discard another's phone mail.
#[test]
fn consume_for_the_wrong_peer_keeps_the_mail() {
    let (_dir, paths) = setup();
    let mut journal = journal(&paths);
    journal.accept(&[mail_update(1)]).unwrap();
    assert!(journal.consume("tg:mint:1", "someone-else").is_err());
    assert_eq!(journal.pending().unwrap().len(), 1);
}

// Catches: opening a FIFO named bot.token blocks the poller forever (missing
// O_NONBLOCK) or reads from an attacker-fed pipe.
#[test]
fn fifo_token_is_refused_without_blocking() {
    use std::os::unix::ffi::OsStrExt;
    let (_dir, paths) = setup();
    let path = paths.file("bot.token");
    std::fs::remove_file(&path).unwrap();
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    let (tx, rx) = std::sync::mpsc::channel();
    let p = paths.clone();
    std::thread::spawn(move || tx.send(p.token().err()).ok());
    let got = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("token() blocked on FIFO");
    assert_eq!(got, Some(Error::PrivateFile));
}

// Catches: a symlinked allowlist or token is followed to a file elsewhere.
#[test]
fn symlinked_secret_files_are_refused() {
    let (_dir, paths) = setup();
    for name in ["bot.token", "allowed_chat_ids"] {
        let real = paths.file(&format!("real-{name}"));
        std::fs::rename(paths.file(name), &real).unwrap();
        std::os::unix::fs::symlink(&real, paths.file(name)).unwrap();
    }
    assert_eq!(paths.token().err(), Some(Error::PrivateFile));
    assert_eq!(paths.allowlist().err(), Some(Error::PrivateFile));
}

// Catches: a token with path/query characters rewrites the request URL
// (e.g. "123:x/../sendMessage?").
#[test]
fn token_with_url_metacharacters_is_refused() {
    for bad in ["1:a/b", "1:a?b=c", "1:a#b", "1:a b", "1:\u{e9}"] {
        let (_dir, paths) = setup();
        private(&paths.file("bot.token"), bad);
        assert_eq!(
            paths.token().err(),
            Some(Error::PrivateFile),
            "accepted {bad:?}"
        );
    }
}

// Catches: group/world-readable token or allowlist accepted.
#[test]
fn group_readable_token_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, paths) = setup();
    std::fs::set_permissions(
        paths.file("bot.token"),
        std::fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    assert_eq!(paths.token().err(), Some(Error::PrivateFile));
}

// Catches: allowlist accepts zero, negative (group), signed, hex or commented
// lines, widening authorization.
#[test]
fn allowlist_rejects_non_private_or_non_decimal_lines() {
    for bad in [
        "0",
        "-100123",
        "+5",
        "0x10",
        "5 # me",
        "abc",
        "99999999999999999999",
    ] {
        let (_dir, paths) = setup();
        private(
            &paths.file("allowed_chat_ids"),
            &format!("{OWNER_CHAT}\n{bad}\n"),
        );
        assert_eq!(
            paths.allowlist().err(),
            Some(Error::Config),
            "accepted {bad:?}"
        );
    }
}
