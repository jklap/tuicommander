//! Pure argv parsing for the tmux compatibility shim.
//!
//! Two layers, mirroring real tmux(1):
//!
//! 1. [`split_globals`] strips tmux's *global* options (`-L`, `-S`, `-V`, ...),
//!    which always precede the subcommand. Claude Code's teammate-pane backend
//!    prefixes every swarm-path call with `-L claude-swarm-<pid>` — without this
//!    pre-pass, `-L` itself would be parsed as the subcommand and every swarm
//!    invocation would die on "unknown command '-L'".
//! 2. [`parse_args`] is a small getopt(3)-alike for one subcommand's own flags,
//!    driven by a per-subcommand [`OptSpec`].
//!
//! Nothing here touches the network or the filesystem.

use std::collections::BTreeSet;

/// Global tmux options recognized ahead of the subcommand.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct GlobalOpts {
    /// `-L <name>` — named socket. Claude Code's swarm path always sets this to
    /// `claude-swarm-<pid>`, which is how concurrent Claude Code processes stay
    /// isolated from each other (the *session* they create is always literally
    /// named `claude-swarm`, so `-L` is the only thing that partitions them).
    pub socket_name: Option<String>,
    /// `-S <path>` — explicit socket path (the "leader" path).
    pub socket_path: Option<String>,
    /// `-V` given as a global option (`tmux -V`, no subcommand).
    pub version: bool,
}

impl GlobalOpts {
    /// The key used to partition shim topology: the `-L` value, else a derived
    /// key from `-S`, else a fixed default for plain `tuic alias` users who
    /// never pass either.
    pub(crate) fn label(&self) -> String {
        if let Some(l) = &self.socket_name {
            return l.clone();
        }
        if let Some(s) = &self.socket_path {
            return format!("S-{:x}", fnv1a(s));
        }
        "default".to_string()
    }
}

/// Tiny dependency-free FNV-1a hash — just needs to be stable and short, not
/// cryptographic.
fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) enum ArgError {
    MissingValue(char),
    UnknownFlag(char),
}

impl std::fmt::Display for ArgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgError::MissingValue(c) => write!(f, "option requires an argument -- '{c}'"),
            ArgError::UnknownFlag(c) => write!(f, "unknown option -- '{c}'"),
        }
    }
}

/// Global flags that consume a value: `-L`, `-S`, `-f` (config file), `-c`
/// (shell command). `-f`/`-c` are accepted so a global-option probe never
/// errors, even though their values aren't modeled.
const GLOBAL_VALUE_FLAGS: &str = "LSfc";
/// Global boolean flags: version probe plus tmux's control/terminal toggles,
/// none of which this shim needs to act on.
const GLOBAL_BOOL_FLAGS: &str = "V2CDluv";

/// The subcommand name plus its own (not-yet-parsed) argv, once globals are
/// stripped.
type SubcommandCall = (String, Vec<String>);

/// Split `argv` (already excluding argv[0]) into `(globals, Some((subcommand,
/// rest)))`, or `(globals, None)` when there is no subcommand — bare `tmux`,
/// or `tmux -V` with nothing after it.
pub(crate) fn split_globals(
    argv: &[String],
) -> Result<(GlobalOpts, Option<SubcommandCall>), ArgError> {
    let mut globals = GlobalOpts::default();
    let mut i = 0;
    while i < argv.len() {
        let tok = argv[i].as_str();
        if tok == "--" {
            return Ok((globals, None));
        }
        if tok == "-" || !tok.starts_with('-') {
            return Ok((globals, Some((tok.to_string(), argv[i + 1..].to_vec()))));
        }
        let chars: Vec<char> = tok[1..].chars().collect();
        let mut j = 0;
        while j < chars.len() {
            let c = chars[j];
            if GLOBAL_VALUE_FLAGS.contains(c) {
                let rest: String = chars[j + 1..].iter().collect();
                let value = if rest.is_empty() {
                    i += 1;
                    argv.get(i).cloned().ok_or(ArgError::MissingValue(c))?
                } else {
                    rest
                };
                match c {
                    'L' => globals.socket_name = Some(value),
                    'S' => globals.socket_path = Some(value),
                    _ => {} // -f, -c: accepted, value intentionally not modeled
                }
                j = chars.len();
            } else if GLOBAL_BOOL_FLAGS.contains(c) {
                if c == 'V' {
                    globals.version = true;
                }
                j += 1;
            } else {
                return Err(ArgError::UnknownFlag(c));
            }
        }
        i += 1;
    }
    Ok((globals, None))
}

/// Per-subcommand flag spec: which short flags take a value, which are plain
/// booleans, and whether the first non-option token ends parsing (tmux's
/// trailing-command subcommands: `new-session`, `new-window`, `split-window`,
/// `respawn-pane`).
#[derive(Clone, Copy)]
pub(crate) struct OptSpec {
    pub value_flags: &'static str,
    pub bool_flags: &'static str,
    pub command_tail: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ParsedArgs {
    values: Vec<(char, String)>,
    flags: BTreeSet<char>,
    positional: Vec<String>,
}

impl ParsedArgs {
    /// Last occurrence wins, matching getopt(3) / tmux's own `-t a -t b`
    /// behavior.
    pub(crate) fn value(&self, f: char) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(k, _)| *k == f)
            .map(|(_, v)| v.as_str())
    }

    pub(crate) fn has(&self, f: char) -> bool {
        self.flags.contains(&f)
    }

    pub(crate) fn positional(&self) -> &[String] {
        &self.positional
    }
}

/// Parse one subcommand's own argv (globals already stripped) against `spec`.
pub(crate) fn parse_args(spec: &OptSpec, argv: &[String]) -> Result<ParsedArgs, ArgError> {
    let mut out = ParsedArgs::default();
    let mut i = 0;
    let mut ended_options = false;
    while i < argv.len() {
        let tok = argv[i].as_str();
        if !ended_options && tok == "--" {
            ended_options = true;
            i += 1;
            continue;
        }
        if !ended_options && tok.len() > 1 && tok.starts_with('-') {
            let chars: Vec<char> = tok[1..].chars().collect();
            let mut j = 0;
            while j < chars.len() {
                let c = chars[j];
                if spec.value_flags.contains(c) {
                    let rest: String = chars[j + 1..].iter().collect();
                    let value = if rest.is_empty() {
                        i += 1;
                        argv.get(i).cloned().ok_or(ArgError::MissingValue(c))?
                    } else {
                        rest
                    };
                    out.values.push((c, value));
                    j = chars.len();
                } else if spec.bool_flags.contains(c) {
                    out.flags.insert(c);
                    j += 1;
                } else {
                    return Err(ArgError::UnknownFlag(c));
                }
            }
            i += 1;
            continue;
        }
        if spec.command_tail {
            out.positional = argv[i..].to_vec();
            return Ok(out);
        }
        out.positional.push(tok.to_string());
        i += 1;
    }
    Ok(out)
}

/// One parsed tmux subcommand invocation, backend-agnostic. `Unknown` and
/// `Noop` both carry the original subcommand name for logging.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TmuxOp {
    Version,
    Bare,
    HasSession {
        target: Option<String>,
    },
    NewSession {
        /// `-s` — the session's own name.
        session_name: Option<String>,
        /// `-n` — the initial window's name. Distinct from `session_name`:
        /// Claude Code's call always sets both (`-s claude-swarm -n
        /// swarm-view`), naming two different objects.
        window_name: Option<String>,
        cwd: Option<String>,
        detached: bool,
        print: bool,
        format: Option<String>,
        command: Vec<String>,
    },
    NewWindow {
        target: Option<String>,
        name: Option<String>,
        cwd: Option<String>,
        print: bool,
        format: Option<String>,
        command: Vec<String>,
    },
    SplitWindow {
        target: Option<String>,
        cwd: Option<String>,
        print: bool,
        format: Option<String>,
        command: Vec<String>,
    },
    RespawnPane {
        target: Option<String>,
        kill: bool,
        command: Vec<String>,
    },
    SelectPane {
        target: Option<String>,
        title: Option<String>,
    },
    SendKeys {
        target: Option<String>,
        keys: Vec<String>,
    },
    DisplayMessage {
        target: Option<String>,
        format: String,
    },
    ListPanes {
        target: Option<String>,
        format: Option<String>,
    },
    ListWindows {
        target: Option<String>,
        format: Option<String>,
    },
    ListSessions,
    CapturePane {
        target: Option<String>,
        lines: Option<usize>,
    },
    ResizePane {
        target: Option<String>,
        x: Option<u16>,
        y: Option<u16>,
    },
    KillPane {
        target: Option<String>,
    },
    KillSession {
        target: Option<String>,
    },
    KillServer,
    AttachSession,
    /// Cosmetic tmux commands with no TUIC-tab equivalent: `select-layout`,
    /// `set-option`/`set`, `set-window-option`/`setw`, `switch-client`.
    /// Always succeeds with no output, regardless of its own flags.
    Noop(String),
    Unknown(String, Vec<String>),
}

/// Parse a full tmux invocation (globals + subcommand) into a `TmuxOp`.
pub(crate) fn parse_tmux(argv: &[String]) -> Result<(GlobalOpts, TmuxOp), ArgError> {
    let (globals, split) = split_globals(argv)?;
    let Some((subcmd, rest)) = split else {
        if globals.version {
            return Ok((globals, TmuxOp::Version));
        }
        return Ok((globals, TmuxOp::Bare));
    };

    let op = match subcmd.as_str() {
        "-V" => TmuxOp::Version,
        "new-session" | "new" => {
            let spec = OptSpec {
                value_flags: "scnF",
                bool_flags: "dP",
                command_tail: true,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::NewSession {
                session_name: p.value('s').map(String::from),
                window_name: p.value('n').map(String::from),
                cwd: p.value('c').map(String::from),
                detached: p.has('d'),
                print: p.has('P'),
                format: p.value('F').map(String::from),
                command: p.positional().to_vec(),
            }
        }
        "new-window" => {
            let spec = OptSpec {
                value_flags: "tcnF",
                bool_flags: "dP",
                command_tail: true,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::NewWindow {
                target: p.value('t').map(String::from),
                name: p.value('n').map(String::from),
                cwd: p.value('c').map(String::from),
                print: p.has('P'),
                format: p.value('F').map(String::from),
                command: p.positional().to_vec(),
            }
        }
        "split-window" => {
            let spec = OptSpec {
                value_flags: "tcFl",
                bool_flags: "dhvPZ",
                command_tail: true,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::SplitWindow {
                target: p.value('t').map(String::from),
                cwd: p.value('c').map(String::from),
                print: p.has('P'),
                format: p.value('F').map(String::from),
                command: p.positional().to_vec(),
            }
        }
        "respawn-pane" => {
            let spec = OptSpec {
                value_flags: "tc",
                bool_flags: "k",
                command_tail: true,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::RespawnPane {
                target: p.value('t').map(String::from),
                kill: p.has('k'),
                command: p.positional().to_vec(),
            }
        }
        "select-pane" => {
            let spec = OptSpec {
                value_flags: "tT",
                bool_flags: "DdEeGgLlMmRUZ",
                command_tail: false,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::SelectPane {
                target: p.value('t').map(String::from),
                title: p.value('T').map(String::from),
            }
        }
        "send-keys" | "send" => {
            let spec = OptSpec {
                value_flags: "tN",
                bool_flags: "FHKMRXl",
                command_tail: false,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::SendKeys {
                target: p.value('t').map(String::from),
                keys: p.positional().to_vec(),
            }
        }
        "display-message" | "display" => {
            let spec = OptSpec {
                value_flags: "tF",
                bool_flags: "acIpv",
                command_tail: false,
            };
            let p = parse_args(&spec, &rest)?;
            // Real tmux also accepts a trailing positional as the format when
            // `-F` is absent; either works as our single format source.
            let format = p
                .value('F')
                .map(String::from)
                .or_else(|| p.positional().first().cloned())
                .unwrap_or_default();
            TmuxOp::DisplayMessage {
                target: p.value('t').map(String::from),
                format,
            }
        }
        "list-panes" | "lsp" => {
            let spec = OptSpec {
                value_flags: "tF",
                bool_flags: "as",
                command_tail: false,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::ListPanes {
                target: p.value('t').map(String::from),
                format: p.value('F').map(String::from),
            }
        }
        "list-windows" | "lsw" => {
            let spec = OptSpec {
                value_flags: "tF",
                bool_flags: "a",
                command_tail: false,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::ListWindows {
                target: p.value('t').map(String::from),
                format: p.value('F').map(String::from),
            }
        }
        "list-sessions" | "ls" => TmuxOp::ListSessions,
        "capture-pane" => {
            let spec = OptSpec {
                value_flags: "tSE",
                bool_flags: "peJ",
                command_tail: false,
            };
            let p = parse_args(&spec, &rest)?;
            // `-S -N` (start N lines back) is the only range form we honor;
            // `-E`/`-J`/`-e` have no equivalent in the /output endpoint today.
            let lines = p
                .value('S')
                .and_then(|s| s.trim_start_matches('-').parse::<usize>().ok());
            TmuxOp::CapturePane {
                target: p.value('t').map(String::from),
                lines,
            }
        }
        "resize-pane" => {
            let spec = OptSpec {
                value_flags: "txy",
                bool_flags: "DLRUZ",
                command_tail: false,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::ResizePane {
                target: p.value('t').map(String::from),
                x: p.value('x').and_then(|v| v.parse().ok()),
                y: p.value('y').and_then(|v| v.parse().ok()),
            }
        }
        "kill-pane" => {
            let spec = OptSpec {
                value_flags: "t",
                bool_flags: "a",
                command_tail: false,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::KillPane {
                target: p.value('t').map(String::from),
            }
        }
        "kill-session" => {
            let spec = OptSpec {
                value_flags: "t",
                bool_flags: "a",
                command_tail: false,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::KillSession {
                target: p.value('t').map(String::from),
            }
        }
        "kill-server" => TmuxOp::KillServer,
        "has-session" | "has" => {
            let spec = OptSpec {
                value_flags: "t",
                bool_flags: "",
                command_tail: false,
            };
            let p = parse_args(&spec, &rest)?;
            TmuxOp::HasSession {
                target: p.value('t').map(String::from),
            }
        }
        "attach-session" | "attach" | "a" => TmuxOp::AttachSession,
        "select-layout" | "set-option" | "set" | "set-window-option" | "setw" | "switch-client"
        | "rename-window" => TmuxOp::Noop(subcmd.clone()),
        other => TmuxOp::Unknown(other.to_string(), rest),
    };
    Ok((globals, op))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn split_globals_extracts_l_flag_before_subcommand() {
        let (globals, split) = split_globals(&s(&[
            "-L",
            "claude-swarm-42",
            "new-session",
            "-d",
            "-s",
            "claude-swarm",
        ]))
        .unwrap();
        assert_eq!(globals.socket_name.as_deref(), Some("claude-swarm-42"));
        assert_eq!(globals.label(), "claude-swarm-42");
        let (subcmd, rest) = split.unwrap();
        assert_eq!(subcmd, "new-session");
        assert_eq!(rest, s(&["-d", "-s", "claude-swarm"]));
    }

    #[test]
    fn split_globals_handles_attached_l_value() {
        let (globals, split) = split_globals(&s(&["-Lclaude-swarm-7", "has-session"])).unwrap();
        assert_eq!(globals.socket_name.as_deref(), Some("claude-swarm-7"));
        assert_eq!(split.unwrap().0, "has-session");
    }

    #[test]
    fn split_globals_s_flag_derives_a_stable_label() {
        let (g1, _) = split_globals(&s(&["-S", "/tmp/sock", "ls"])).unwrap();
        let (g2, _) = split_globals(&s(&["-S", "/tmp/sock", "ls"])).unwrap();
        assert_eq!(g1.label(), g2.label());
        assert_ne!(g1.label(), "default");
    }

    #[test]
    fn split_globals_with_no_subcommand_is_bare_or_version() {
        assert_eq!(split_globals(&s(&[])).unwrap().1, None);
        let (g, split) = split_globals(&s(&["-V"])).unwrap();
        assert!(g.version);
        assert_eq!(split, None);
    }

    #[test]
    fn split_globals_missing_value_errors() {
        assert_eq!(
            split_globals(&s(&["-L"])).unwrap_err(),
            ArgError::MissingValue('L')
        );
    }

    #[test]
    fn split_globals_unknown_global_flag_errors() {
        assert_eq!(
            split_globals(&s(&["-Q", "ls"])).unwrap_err(),
            ArgError::UnknownFlag('Q')
        );
    }

    #[test]
    fn parse_tmux_full_swarm_new_session_call() {
        let (globals, op) = parse_tmux(&s(&[
            "-L",
            "claude-swarm-42",
            "new-session",
            "-d",
            "-s",
            "claude-swarm",
            "-n",
            "swarm-view",
            "-P",
            "-F",
            "#{pane_id}",
            "--",
            "cat",
        ]))
        .unwrap();
        assert_eq!(globals.label(), "claude-swarm-42");
        assert_eq!(
            op,
            TmuxOp::NewSession {
                session_name: Some("claude-swarm".to_string()),
                window_name: Some("swarm-view".to_string()),
                cwd: None,
                detached: true,
                print: true,
                format: Some("#{pane_id}".to_string()),
                command: vec!["cat".to_string()],
            }
        );
    }

    #[test]
    fn parse_args_clustered_booleans() {
        let spec = OptSpec {
            value_flags: "tF",
            bool_flags: "dP",
            command_tail: true,
        };
        let p = parse_args(&spec, &s(&["-dP", "-t", "%3"])).unwrap();
        assert!(p.has('d'));
        assert!(p.has('P'));
        assert_eq!(p.value('t'), Some("%3"));
    }

    #[test]
    fn parse_args_cluster_ending_in_value_flag() {
        let spec = OptSpec {
            value_flags: "t",
            bool_flags: "d",
            command_tail: true,
        };
        let p = parse_args(&spec, &s(&["-dt", "%3"])).unwrap();
        assert!(p.has('d'));
        assert_eq!(p.value('t'), Some("%3"));

        let p2 = parse_args(&spec, &s(&["-dt%3"])).unwrap();
        assert!(p2.has('d'));
        assert_eq!(p2.value('t'), Some("%3"));
    }

    #[test]
    fn parse_args_missing_value_at_end_errors() {
        let spec = OptSpec {
            value_flags: "t",
            bool_flags: "",
            command_tail: false,
        };
        assert_eq!(
            parse_args(&spec, &s(&["-t"])).unwrap_err(),
            ArgError::MissingValue('t')
        );
    }

    #[test]
    fn parse_args_repeated_flag_last_wins() {
        let spec = OptSpec {
            value_flags: "t",
            bool_flags: "",
            command_tail: false,
        };
        let p = parse_args(&spec, &s(&["-t", "a", "-t", "b"])).unwrap();
        assert_eq!(p.value('t'), Some("b"));
    }

    #[test]
    fn parse_args_command_tail_keeps_repeated_value_as_positional() {
        // Regression pinned from the pre-refactor `without_flag` behavior:
        // `send-keys -t build build Enter` must still send the literal
        // "build" even though it equals the target's own value.
        let spec = OptSpec {
            value_flags: "t",
            bool_flags: "",
            command_tail: false,
        };
        let p = parse_args(&spec, &s(&["-t", "build", "build", "Enter"])).unwrap();
        assert_eq!(p.value('t'), Some("build"));
        assert_eq!(p.positional(), &s(&["build", "Enter"]));
    }

    #[test]
    fn parse_args_unknown_flag_errors_in_strict_mode() {
        let spec = OptSpec {
            value_flags: "t",
            bool_flags: "",
            command_tail: false,
        };
        assert_eq!(
            parse_args(&spec, &s(&["-l", "foo"])).unwrap_err(),
            ArgError::UnknownFlag('l')
        );
    }

    #[test]
    fn send_keys_classifies_l_as_a_boolean_flag_not_a_target_value() {
        // The pre-refactor bug: without_flag(rest, "-t") didn't know `-l` was
        // a flag, so `send-keys -t x -l foo` typed the literal "-l foo".
        let (_, op) = parse_tmux(&s(&["send-keys", "-t", "x", "-l", "foo"])).unwrap();
        assert_eq!(
            op,
            TmuxOp::SendKeys {
                target: Some("x".to_string()),
                keys: vec!["foo".to_string()],
            }
        );
    }

    #[test]
    fn split_window_l_is_a_value_flag_not_boolean() {
        // Proof the spec is genuinely per-subcommand: -l is boolean on
        // send-keys but takes a value (a size) on split-window.
        let (_, op) = parse_tmux(&s(&["split-window", "-l", "70%", "-t", "%2"])).unwrap();
        assert_eq!(
            op,
            TmuxOp::SplitWindow {
                target: Some("%2".to_string()),
                cwd: None,
                print: false,
                format: None,
                command: vec![],
            }
        );
    }

    #[test]
    fn missing_t_on_has_session_leaves_target_none_rather_than_empty_string() {
        let (_, op) = parse_tmux(&s(&["has-session"])).unwrap();
        assert_eq!(op, TmuxOp::HasSession { target: None });
    }

    #[test]
    fn set_option_variants_are_noop_regardless_of_flags() {
        for (argv, name) in [
            (
                s(&["set-option", "-p", "-t", "%3", "window-style", "bg=red"]),
                "set-option",
            ),
            (
                s(&["set-window-option", "-t", "@1", "pane-border-status", "top"]),
                "set-window-option",
            ),
            (
                s(&["select-layout", "-t", "claude-swarm:swarm-view", "tiled"]),
                "select-layout",
            ),
            (s(&["switch-client", "-t", "claude-swarm"]), "switch-client"),
        ] {
            let (_, op) = parse_tmux(&argv).unwrap();
            assert_eq!(op, TmuxOp::Noop(name.to_string()));
        }
    }

    #[test]
    fn resize_pane_zoom_alone_carries_no_dimensions() {
        let (_, op) = parse_tmux(&s(&["resize-pane", "-Z", "-t", "%1"])).unwrap();
        assert_eq!(
            op,
            TmuxOp::ResizePane {
                target: Some("%1".to_string()),
                x: None,
                y: None,
            }
        );
    }

    #[test]
    fn unknown_subcommand_is_carried_verbatim_for_logging() {
        let (_, op) = parse_tmux(&s(&["totally-made-up", "-x", "1"])).unwrap();
        assert_eq!(
            op,
            TmuxOp::Unknown("totally-made-up".to_string(), s(&["-x", "1"]))
        );
    }

    #[test]
    fn respawn_pane_kill_flag_and_trailing_command() {
        let (_, op) =
            parse_tmux(&s(&["respawn-pane", "-k", "-t", "%3", "--", "echo", "hi"])).unwrap();
        assert_eq!(
            op,
            TmuxOp::RespawnPane {
                target: Some("%3".to_string()),
                kill: true,
                command: vec!["echo".to_string(), "hi".to_string()],
            }
        );
    }

    #[test]
    fn bare_tmux_with_no_args_is_bare() {
        let (globals, op) = parse_tmux(&s(&[])).unwrap();
        assert!(!globals.version);
        assert_eq!(op, TmuxOp::Bare);
    }
}
