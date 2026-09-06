//! Shared setup for the ACP client tests.
//!
//! Every ACP test needs the same three things: a root directory, a scenario
//! inside it for the fixture agent to run, and a manager pointed at that agent.
//! They are here so a scenario name is the only thing a test has to say, and so
//! the root outlives the manager — a `TempDir` dropped early deletes the
//! directory the child is still running in, which fails as a protocol error
//! somewhere far from the cause.

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use tuicommander_lib::acp::{
    AcpClientManager, AcpConnectRequest, AcpConnectionSnapshot, EgoAcpConfig,
};

pub struct Fixture {
    root: TempDir,
    pub manager: AcpClientManager,
}

impl Fixture {
    /// A manager whose agent will run `tests/fixtures/acp/<scenario>.jsonl`.
    pub fn with(scenario: &str) -> Self {
        let root = TempDir::new().expect("ACP root");
        std::fs::copy(
            Path::new("tests/fixtures/acp").join(format!("{scenario}.jsonl")),
            root.path().join("scenario.jsonl"),
        )
        .unwrap_or_else(|error| panic!("scenario {scenario}: {error}"));
        Self {
            root,
            manager: AcpClientManager::new(EgoAcpConfig {
                executable: fixture_agent(),
            }),
        }
    }

    pub fn root(&self) -> PathBuf {
        self.root.path().to_path_buf()
    }

    pub async fn connect(&self) -> AcpConnectionSnapshot {
        self.manager
            .connect(AcpConnectRequest { root: self.root() })
            .await
            .expect("connect")
    }
}

#[must_use]
pub fn fixture_agent() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tuic-acp-fixture-agent"))
}
