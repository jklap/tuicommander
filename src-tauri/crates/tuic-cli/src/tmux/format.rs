//! `#{...}` format-string rendering for `-F`/`-p` flags.
//!
//! Only the variables Claude Code's teammate-pane backend actually references
//! are supported (recovered from the installed binary): `pane_id`,
//! `window_id`, `window_name`, `session_name`, `pane_title`,
//! `client_termtype`, `client_control_mode`. Unknown variables render empty,
//! matching real tmux — never the literal placeholder, since Claude Code
//! would happily adopt `#{pane_id}` itself as a target and then track a pane
//! that was never created.

/// The values one format string can draw from. Any field left `None` renders
/// as `""` for every variable that depends on it.
#[derive(Debug, Default, Clone)]
pub(crate) struct FormatCtx {
    pub session_name: Option<String>,
    pub window_id: Option<String>,
    pub window_name: Option<String>,
    pub pane_id: Option<String>,
    pub pane_title: Option<String>,
}

fn lookup(name: &str, ctx: &FormatCtx) -> String {
    match name {
        "pane_id" => ctx.pane_id.clone().unwrap_or_default(),
        "window_id" => ctx.window_id.clone().unwrap_or_default(),
        "window_name" => ctx.window_name.clone().unwrap_or_default(),
        "session_name" => ctx.session_name.clone().unwrap_or_default(),
        "pane_title" => ctx.pane_title.clone().unwrap_or_default(),
        // Rendering a real value here would override Claude Code's own
        // XTVERSION-derived terminal-identity detection — leave it empty so
        // its own detection stays authoritative.
        "client_termtype" => String::new(),
        "client_control_mode" => "0".to_string(),
        _ => String::new(),
    }
}

/// Render one format string. Supports `#{var}`, `##` (literal `#`), and
/// passes through an unterminated `#{` literally rather than panicking.
pub(crate) fn render_format(fmt: &str, ctx: &FormatCtx) -> String {
    let mut out = String::with_capacity(fmt.len());
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '#' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('#') => {
                chars.next();
                out.push('#');
            }
            Some('{') => {
                chars.next(); // consume '{'
                let mut name = String::new();
                let mut closed = false;
                for nc in chars.by_ref() {
                    if nc == '}' {
                        closed = true;
                        break;
                    }
                    name.push(nc);
                }
                if closed {
                    out.push_str(&lookup(name.trim(), ctx));
                } else {
                    // Unterminated `#{...` — pass through literally.
                    out.push('#');
                    out.push('{');
                    out.push_str(&name);
                }
            }
            _ => out.push('#'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> FormatCtx {
        FormatCtx {
            session_name: Some("claude-swarm".to_string()),
            window_id: Some("@1".to_string()),
            window_name: Some("swarm-view".to_string()),
            pane_id: Some("%3".to_string()),
            pane_title: Some("teammate-1".to_string()),
        }
    }

    #[test]
    fn renders_the_required_swarm_variables() {
        assert_eq!(render_format("#{pane_id}", &ctx()), "%3");
        assert_eq!(render_format("#{window_id}", &ctx()), "@1");
        assert_eq!(render_format("#{window_name}", &ctx()), "swarm-view");
        assert_eq!(render_format("#{session_name}", &ctx()), "claude-swarm");
        assert_eq!(render_format("#{pane_title}", &ctx()), "teammate-1");
        assert_eq!(render_format("#{client_termtype}", &ctx()), "");
        assert_eq!(render_format("#{client_control_mode}", &ctx()), "0");
    }

    #[test]
    fn renders_a_compound_target_string() {
        assert_eq!(
            render_format("#{session_name}:#{window_id}.#{pane_id}", &ctx()),
            "claude-swarm:@1.%3"
        );
    }

    #[test]
    fn unknown_variable_renders_empty_never_the_literal() {
        assert_eq!(render_format("#{totally_made_up}", &ctx()), "");
    }

    #[test]
    fn double_hash_is_a_literal_hash() {
        assert_eq!(render_format("50##done", &ctx()), "50#done");
    }

    #[test]
    fn unterminated_brace_passes_through_literally() {
        assert_eq!(render_format("#{pane_id", &ctx()), "#{pane_id");
    }

    #[test]
    fn absent_field_renders_empty() {
        let empty = FormatCtx::default();
        assert_eq!(render_format("#{pane_id}", &empty), "");
    }

    #[test]
    fn plain_text_with_no_hash_is_unchanged() {
        assert_eq!(render_format("plain text", &ctx()), "plain text");
    }

    #[test]
    fn empty_format_is_empty() {
        assert_eq!(render_format("", &ctx()), "");
    }
}
