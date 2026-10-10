use super::*;
use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

pub(in crate::design_mode) fn git(repo: &Path, args: &[&str]) {
    let result = Command::new(crate::cli::resolve_cli("git"))
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
pub(in crate::design_mode) fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    git(dir.path(), &["init"]);
    git(
        dir.path(),
        &[
            "-c",
            "user.name=Design test",
            "-c",
            "user.email=design@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
    dir
}
pub(in crate::design_mode) struct Server {
    child: Child,
    pub(in crate::design_mode) port: u16,
}
impl Server {
    pub(in crate::design_mode) fn start(cwd: &Path, script: Option<&Path>) -> Self {
        // OS-owned listener and cwd/argv come from a real process, as in the recorded Mac fixtures.
        let code = "import socket,sys; s=socket.socket(); s.bind(('127.0.0.1',0)); s.listen(); print(s.getsockname()[1],flush=True); sys.stdin.read()";
        let mut command = Command::new("python3");
        command.arg("-u");
        if let Some(script) = script {
            std::fs::write(script, code).unwrap();
            command.arg(script);
        } else {
            command.args(["-c", code]);
        }
        let mut child = command
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        Self {
            child,
            port: line.trim().parse().unwrap(),
        }
    }
    pub(in crate::design_mode) fn pid(&self) -> u32 {
        self.child.id()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub(in crate::design_mode) async fn offered(
    state: Arc<crate::AppState>,
    repo: &Path,
) -> Vec<Value> {
    let response = super::super::extension::tests::request(
        state,
        json!({"version":"2.0.0","action":"list_dev_servers","repoPath":repo}),
    )
    .await;
    assert_eq!(response["ok"], true, "{response}");
    response["result"]["servers"].as_array().unwrap().clone()
}
// Catches: discovery dropping live servers, choosing a parent over the deepest worktree,
// retaining a dead process, or treating a sibling path prefix as repository ownership.
#[tokio::test]
async fn recorded_mac_listeners_use_deepest_cwd_and_require_a_live_owner() {
    let repo = repository();
    let nested = repo.path().join("app");
    git(
        repo.path(),
        &["worktree", "add", "--detach", nested.to_str().unwrap()],
    );
    let sibling = repo.path().with_extension("other");
    std::fs::create_dir(&sibling).unwrap();
    let parent = Server::start(repo.path(), None);
    let child = Server::start(&nested, None);
    let foreign = Server::start(&sibling, None);
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let found = offered(state.clone(), repo.path()).await;
    for (server, root) in [(&parent, repo.path()), (&child, nested.as_path())] {
        let row = found
            .iter()
            .find(|row| row["pid"] == server.pid())
            .expect("live repository server was not offered");
        assert_eq!(row["url"], format!("http://localhost:{}/", server.port));
        assert_eq!(row["worktreePath"], json!(root));
    }
    assert!(!found.iter().any(|row| row["pid"] == foreign.pid()));
    let dead_pid = child.pid();
    drop(child);
    assert!(
        !offered(state, repo.path())
            .await
            .iter()
            .any(|row| row["pid"] == dead_pid)
    );
    drop(foreign);
    std::fs::remove_dir(&sibling).unwrap();
}
// Catches: public discovery losing a linked worktree or failing to use an absolute
// Vite-like argv path when cwd is outside the repository; sibling prefixes cannot claim it.
#[tokio::test]
async fn recorded_vite_processes_distinguish_worktrees_and_command_path_boundaries() {
    let repo = repository();
    let worktree = repo.path().join("linked");
    git(
        repo.path(),
        &["worktree", "add", "--detach", worktree.to_str().unwrap()],
    );
    let outside = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let linked = Server::start(&worktree, None);
    let fallback = Server::start(outside.path(), Some(&repo.path().join("vite.py")));
    let foreign = Server::start(
        outside.path(),
        Some(&repo.path().with_extension("other.py")),
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let found = offered(state, repo.path()).await;
    for (server, root) in [
        (&linked, &worktree),
        (&fallback, &repo.path().to_path_buf()),
    ] {
        let row = found
            .iter()
            .find(|row| row["pid"] == server.pid())
            .expect("owned server absent from discovery response");
        assert_eq!(row["worktreePath"], json!(root));
        assert_eq!(row["url"], format!("http://localhost:{}/", server.port));
    }
    assert!(!found.iter().any(|row| row["pid"] == foreign.pid()));
    drop(foreign);
    std::fs::remove_file(repo.path().with_extension("other.py")).unwrap();
}
