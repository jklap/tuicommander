//! Pure tmux `-t` target parsing and resolution against a fetched topology
//! snapshot (`GET /tmux/topology?label=`'s JSON body — see
//! `src-tauri/src/mcp_http/tmux_routes.rs` for the server-side shape this
//! mirrors).
//!
//! Parsing is pure and needs no I/O. Resolution against a topology snapshot
//! is also pure — the snapshot itself is fetched once by the caller.

use serde_json::Value;

/// One parsed `-t` target string.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Target<'a> {
    Pane(&'a str),
    Window(&'a str),
    Session(&'a str),
    /// `session[:window[.pane]]`, any component a name, index, or absent —
    /// e.g. `claude-swarm`, `claude-swarm:swarm-view`,
    /// `claude-swarm:swarm-view.1`, `:swarm-view.1`, `.1`.
    Path {
        session: Option<&'a str>,
        window: Option<&'a str>,
        pane: Option<&'a str>,
    },
}

pub(crate) fn parse_target(s: &str) -> Target<'_> {
    if let Some(rest) = s.strip_prefix('%') {
        return Target::Pane(rest);
    }
    if let Some(rest) = s.strip_prefix('@') {
        return Target::Window(rest);
    }
    if let Some(rest) = s.strip_prefix('$') {
        return Target::Session(rest);
    }
    // With a ':' present: session:window.pane, split on the FIRST ':' then
    // the LAST '.' (a window name may itself contain a dot; tmux's own
    // pane-index suffix never does).
    if let Some((session, win_pane)) = s.split_once(':') {
        let (window_part, pane_part) = match win_pane.rsplit_once('.') {
            Some((win, pane)) => (
                if win.is_empty() { None } else { Some(win) },
                if pane.is_empty() { None } else { Some(pane) },
            ),
            None => (
                if win_pane.is_empty() {
                    None
                } else {
                    Some(win_pane)
                },
                None,
            ),
        };
        return Target::Path {
            session: if session.is_empty() {
                None
            } else {
                Some(session)
            },
            window: window_part,
            pane: pane_part,
        };
    }

    // No ':' at all: NOT window.pane — tmux's target grammar only splits on
    // '.' once a ':' has established there's a window component. A bare
    // word here (`-t claude-swarm`) is a session name, the common case for
    // this shim's callers (has-session/kill-session/new-window all take a
    // bare session name). The one exception is a leading '.', tmux's
    // pane-index-only shorthand (`-t .1`, meaning "pane 1 of the current
    // window") — checked first so it isn't swallowed as a session named ".1".
    if let Some(pane) = s.strip_prefix('.') {
        return Target::Path {
            session: None,
            window: None,
            pane: if pane.is_empty() { None } else { Some(pane) },
        };
    }
    Target::Path {
        session: if s.is_empty() { None } else { Some(s) },
        window: None,
        pane: None,
    }
}

fn find_session<'v>(topology: &'v Value, session: &str) -> Option<&'v Value> {
    topology
        .get("sessions")?
        .as_array()?
        .iter()
        .find(|s| s["id"].as_str() == Some(session) || s["name"].as_str() == Some(session))
}

fn find_window<'v>(session: &'v Value, window: &str) -> Option<&'v Value> {
    session.get("windows")?.as_array()?.iter().find(|w| {
        w["id"].as_str() == Some(window)
            || w["name"].as_str() == Some(window)
            || w["index"].as_u64().map(|i| i.to_string()) == Some(window.to_string())
    })
}

fn find_pane_in_window<'v>(window: &'v Value, pane: &str) -> Option<&'v Value> {
    window.get("panes")?.as_array()?.iter().find(|p| {
        p["id"].as_str() == Some(pane)
            || p["index"].as_u64().map(|i| i.to_string()) == Some(pane.to_string())
    })
}

fn find_pane_by_id<'v>(topology: &'v Value, pane_id: &str) -> Option<&'v Value> {
    for session in topology.get("sessions")?.as_array()? {
        for window in session.get("windows")?.as_array()? {
            if let Some(p) = find_pane_in_window(window, pane_id) {
                return Some(p);
            }
        }
    }
    None
}

fn active_pane_id(window: &Value) -> Option<&str> {
    let active = window["active_pane"].as_str();
    if let Some(a) = active {
        return Some(a);
    }
    window["panes"].as_array()?.first()?["id"].as_str()
}

fn active_window(session: &Value) -> Option<&Value> {
    if let Some(active_id) = session["active_window"].as_str()
        && let Some(w) = find_window(session, active_id)
    {
        return Some(w);
    }
    session["windows"].as_array()?.first()
}

/// Resolve a parsed target down to a concrete session object, the coarsest
/// resolution — used by `new-window`'s `-t` (which names the session/window
/// to add a window to, never a pane).
pub(crate) fn resolve_session<'v>(topology: &'v Value, target: &Target<'_>) -> Option<&'v Value> {
    match target {
        Target::Pane(id) => {
            // A pane target still resolves up to its owning session (defensive;
            // real tmux invocations never target new-window by a pane id).
            topology.get("sessions")?.as_array()?.iter().find(|s| {
                s["windows"].as_array().is_some_and(|ws| {
                    ws.iter()
                        .any(|w| find_pane_in_window(w, &format!("%{id}")).is_some())
                })
            })
        }
        Target::Window(id) => {
            topology.get("sessions")?.as_array()?.iter().find(|s| {
                find_window(s, &format!("@{id}")).is_some() || find_window(s, id).is_some()
            })
        }
        Target::Session(id) => {
            find_session(topology, &format!("${id}")).or_else(|| find_session(topology, id))
        }
        Target::Path { session, .. } => match session {
            Some(s) => find_session(topology, s),
            None => topology.get("sessions")?.as_array()?.first(),
        },
    }
}

/// Resolve a parsed target down to a concrete window object — used by
/// `split-window`'s `-t` (the window a new pane is added to).
pub(crate) fn resolve_window<'v>(topology: &'v Value, target: &Target<'_>) -> Option<&'v Value> {
    match target {
        Target::Pane(id) => {
            for session in topology.get("sessions")?.as_array()? {
                for window in session.get("windows")?.as_array()? {
                    if find_pane_in_window(window, &format!("%{id}")).is_some() {
                        return Some(window);
                    }
                }
            }
            None
        }
        Target::Window(id) => topology
            .get("sessions")?
            .as_array()?
            .iter()
            .find_map(|s| find_window(s, &format!("@{id}")).or_else(|| find_window(s, id))),
        Target::Session(id) => {
            let session =
                find_session(topology, &format!("${id}")).or_else(|| find_session(topology, id))?;
            active_window(session)
        }
        Target::Path {
            session, window, ..
        } => {
            let session_obj = match session {
                Some(s) => find_session(topology, s)?,
                None => topology.get("sessions")?.as_array()?.first()?,
            };
            match window {
                Some(w) => find_window(session_obj, w),
                None => active_window(session_obj),
            }
        }
    }
}

/// Resolve a parsed target down to a concrete pane object in `topology`, or
/// `None` if nothing matches. Callers read `pane["id"]`/`pane["tuic_session_id"]`.
pub(crate) fn resolve_pane<'v>(topology: &'v Value, target: &Target<'_>) -> Option<&'v Value> {
    match target {
        Target::Pane(id) => find_pane_by_id(topology, &format!("%{id}")).or_else(|| {
            // Also accept a bare id already carrying the sigil (defensive —
            // callers always pass the sigil-stripped form today).
            find_pane_by_id(topology, id)
        }),
        Target::Window(id) => {
            let window =
                topology.get("sessions")?.as_array()?.iter().find_map(|s| {
                    find_window(s, &format!("@{id}")).or_else(|| find_window(s, id))
                })?;
            find_pane_in_window(window, active_pane_id(window)?)
        }
        Target::Session(id) => {
            let session =
                find_session(topology, &format!("${id}")).or_else(|| find_session(topology, id))?;
            let window = active_window(session)?;
            find_pane_in_window(window, active_pane_id(window)?)
        }
        Target::Path {
            session,
            window,
            pane,
        } => {
            let session_obj = match session {
                Some(s) => find_session(topology, s)?,
                None => topology.get("sessions")?.as_array()?.first()?,
            };
            let window_obj = match window {
                Some(w) => find_window(session_obj, w)?,
                None => active_window(session_obj)?,
            };
            match pane {
                Some(p) => find_pane_in_window(window_obj, p),
                None => find_pane_in_window(window_obj, active_pane_id(window_obj)?),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_target_recognizes_sigils() {
        assert_eq!(parse_target("%3"), Target::Pane("3"));
        assert_eq!(parse_target("@1"), Target::Window("1"));
        assert_eq!(parse_target("$0"), Target::Session("0"));
    }

    #[test]
    fn parse_target_splits_session_window_pane() {
        assert_eq!(
            parse_target("claude-swarm:swarm-view.1"),
            Target::Path {
                session: Some("claude-swarm"),
                window: Some("swarm-view"),
                pane: Some("1"),
            }
        );
        assert_eq!(
            parse_target("claude-swarm:swarm-view"),
            Target::Path {
                session: Some("claude-swarm"),
                window: Some("swarm-view"),
                pane: None,
            }
        );
        assert_eq!(
            parse_target("claude-swarm"),
            Target::Path {
                session: Some("claude-swarm"),
                window: None,
                pane: None,
            }
        );
        assert_eq!(
            parse_target(":swarm-view.1"),
            Target::Path {
                session: None,
                window: Some("swarm-view"),
                pane: Some("1"),
            }
        );
        assert_eq!(
            parse_target(".1"),
            Target::Path {
                session: None,
                window: None,
                pane: Some("1"),
            }
        );
    }

    fn sample_topology() -> Value {
        json!({
            "sessions": [{
                "id": "$0",
                "name": "claude-swarm",
                "active_window": "@0",
                "windows": [{
                    "id": "@0",
                    "name": "swarm-view",
                    "index": 0,
                    "active_pane": "%1",
                    "panes": [
                        { "id": "%0", "title": "swarm-view", "tuic_session_id": null },
                        { "id": "%1", "title": "teammate-1", "tuic_session_id": "uuid-1" }
                    ]
                }]
            }]
        })
    }

    #[test]
    fn resolve_pane_by_id() {
        let t = sample_topology();
        let target = parse_target("%1");
        let pane = resolve_pane(&t, &target).unwrap();
        assert_eq!(pane["tuic_session_id"], "uuid-1");
    }

    #[test]
    fn resolve_pane_by_path_falls_back_to_active_pane() {
        let t = sample_topology();
        let target = parse_target("claude-swarm:swarm-view");
        let pane = resolve_pane(&t, &target).unwrap();
        assert_eq!(pane["id"], "%1"); // active_pane, not the first pane
    }

    #[test]
    fn resolve_pane_by_bare_session_name() {
        let t = sample_topology();
        let target = parse_target("claude-swarm");
        let pane = resolve_pane(&t, &target).unwrap();
        assert_eq!(pane["id"], "%1");
    }

    #[test]
    fn resolve_pane_returns_none_for_unknown_target() {
        let t = sample_topology();
        let target = parse_target("%99");
        assert!(resolve_pane(&t, &target).is_none());
    }

    #[test]
    fn resolve_window_target_uses_its_active_pane() {
        let t = sample_topology();
        let target = parse_target("@0");
        let pane = resolve_pane(&t, &target).unwrap();
        assert_eq!(pane["id"], "%1");
    }

    #[test]
    fn resolve_window_by_session_name_finds_active_window() {
        let t = sample_topology();
        let target = parse_target("claude-swarm");
        let window = resolve_window(&t, &target).unwrap();
        assert_eq!(window["id"], "@0");
    }

    #[test]
    fn resolve_window_by_pane_id_finds_owning_window() {
        let t = sample_topology();
        let target = parse_target("%1");
        let window = resolve_window(&t, &target).unwrap();
        assert_eq!(window["id"], "@0");
    }

    #[test]
    fn resolve_session_by_window_id_finds_owning_session() {
        let t = sample_topology();
        let target = parse_target("@0");
        let session = resolve_session(&t, &target).unwrap();
        assert_eq!(session["id"], "$0");
    }

    #[test]
    fn resolve_session_by_bare_name() {
        let t = sample_topology();
        let target = parse_target("claude-swarm");
        let session = resolve_session(&t, &target).unwrap();
        assert_eq!(session["id"], "$0");
    }

    #[test]
    fn resolve_session_returns_none_for_unknown_name() {
        let t = sample_topology();
        let target = parse_target("no-such-session");
        assert!(resolve_session(&t, &target).is_none());
    }
}
