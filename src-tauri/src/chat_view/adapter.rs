//! Source-specific parsing; all adapters emit the same ACP updates.

use serde_json::Value;

use super::claude::{AdapterStats, ClaudeAdapter};
use super::codex::CodexAdapter;

pub(super) enum TranscriptAdapter {
    Claude(ClaudeAdapter),
    Codex(CodexAdapter),
}

impl Default for TranscriptAdapter {
    fn default() -> Self {
        Self::Claude(ClaudeAdapter::default())
    }
}

impl TranscriptAdapter {
    pub(super) fn for_agent(agent: &str) -> Self {
        match agent {
            "codex" => Self::Codex(CodexAdapter::default()),
            _ => Self::default(),
        }
    }

    pub(super) fn absorb(&mut self, line: &str) -> Vec<Value> {
        match self {
            Self::Claude(adapter) => adapter.absorb(line),
            Self::Codex(adapter) => adapter.absorb(line),
        }
    }

    pub(super) fn stats(&self) -> AdapterStats {
        match self {
            Self::Claude(adapter) => adapter.stats,
            Self::Codex(adapter) => adapter.stats,
        }
    }

    pub(super) fn reset(&mut self) {
        *self = match self {
            Self::Claude(_) => Self::default(),
            Self::Codex(_) => Self::for_agent("codex"),
        };
    }
}
