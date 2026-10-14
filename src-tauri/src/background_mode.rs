//! Close policy is independent of the window transport.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CloseBehavior {
    #[default]
    Ask,
    Background,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CloseAction {
    Hide,
    Quit,
    Ask,
    ConfirmQuit,
}

pub(crate) fn decide(behavior: CloseBehavior, sessions: usize, confirm: bool) -> CloseAction {
    match behavior {
        CloseBehavior::Background => CloseAction::Hide,
        CloseBehavior::Ask => CloseAction::Ask,
        CloseBehavior::Quit if confirm && sessions > 0 => CloseAction::ConfirmQuit,
        CloseBehavior::Quit => CloseAction::Quit,
    }
}

pub(crate) fn window_close(
    label: &str,
    behavior: CloseBehavior,
    sessions: usize,
    confirm: bool,
) -> Option<CloseAction> {
    (label == "main").then(|| decide(behavior, sessions, confirm))
}

pub(crate) fn prevent_exit(code: Option<i32>, hidden: bool) -> bool {
    code.is_none() && hidden
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CloseChoice {
    Background,
    Quit,
    Cancel,
}

/// Only a pending Ask can accept Background or remember a policy.
#[derive(Default)]
pub(crate) struct CloseState {
    pub hidden: bool,
    pub quitting: bool,
    pub pending: Option<CloseAction>,
}

impl CloseState {
    pub fn resolve(
        &mut self,
        choice: CloseChoice,
        remember: bool,
    ) -> Result<(Option<CloseAction>, Option<CloseBehavior>), String> {
        let pending = self.pending.ok_or("No close dialog is pending")?;
        let action = match choice {
            CloseChoice::Cancel => None,
            CloseChoice::Background if pending == CloseAction::Ask => Some(CloseAction::Hide),
            CloseChoice::Background => {
                return Err("Quit confirmation cannot select background".into());
            }
            CloseChoice::Quit => Some(CloseAction::Quit),
        };
        let remembered = if remember && pending == CloseAction::Ask {
            match choice {
                CloseChoice::Background => Some(CloseBehavior::Background),
                CloseChoice::Quit => Some(CloseBehavior::Quit),
                CloseChoice::Cancel => None,
            }
        } else {
            None
        };
        // Clear only after persistence succeeds (the caller resolves a copy).
        self.pending = None;
        Ok((action, remembered))
    }
}

#[cfg(feature = "desktop")]
#[path = "background_mode_desktop.rs"]
pub(crate) mod desktop;
