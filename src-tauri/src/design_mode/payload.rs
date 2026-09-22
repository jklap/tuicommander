use serde_json::Value;
use std::path::Path;

const MAX_PROMPT_BYTES: usize = 8 * 1024;
const MAX_SNIPPET_BYTES: usize = 2 * 1024;
const STYLE_PROPS: &[&str] = &[
    "color",
    "backgroundColor",
    "fontFamily",
    "fontSize",
    "fontWeight",
    "display",
    "position",
    "margin",
    "padding",
    "border",
    "width",
    "height",
];

// DEFERRED (2026-09-23) — design-grabs needs a retention policy once real usage
// shows a useful age or size limit. Deleting captures earlier could break notes.
pub(crate) struct GrabPayload {
    url: Option<String>,
    selector: String,
    element_path: String,
    full_path: String,
    html_snippet: String,
    nearby_text: String,
    attributes: Vec<(String, String)>,
    styles: Vec<(String, String)>,
    rect: Vec<(String, String)>,
}

fn strip_controls(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

fn clamp(value: &str, max: usize) -> String {
    let clean = strip_controls(value);
    let end = clean.floor_char_boundary(max.min(clean.len()));
    clean[..end].to_owned()
}

fn safe_url(value: &str) -> Option<String> {
    if value == "about:blank" {
        return Some(value.to_owned());
    }
    let mut parsed = url::Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https" | "file") {
        return None;
    }
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    Some(clamp(parsed.as_str(), 512))
}

fn value<'a>(raw: &'a Value, key: &str) -> &'a str {
    raw.get(key).and_then(Value::as_str).unwrap_or("")
}

fn fields(raw: &Value, max: usize, filter: impl Fn(&str, &str) -> bool) -> Vec<(String, String)> {
    raw.as_object()
        .into_iter()
        .flat_map(|map| map.iter())
        .take(32)
        .filter_map(|(key, value)| {
            let text = value.as_str()?;
            if !filter(key, text) {
                return None;
            }
            Some((clamp(key, 64), clamp(text, max)))
        })
        .collect()
}

fn safe_snippet(raw: &Value) -> String {
    let tag = value(raw, "tagName");
    let tag = if !tag.is_empty()
        && tag.len() <= 32
        && tag
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        tag
    } else {
        "element"
    };
    let text = clamp(value(raw, "textContent"), MAX_SNIPPET_BYTES)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<{tag}>{text}</{tag}>")
}

impl GrabPayload {
    pub(crate) fn from_raw(raw: Value, styles: Value, rect: Value) -> Self {
        let attributes = fields(&raw["attributes"], 256, |key, value| {
            matches!(
                key,
                "id" | "class"
                    | "href"
                    | "src"
                    | "alt"
                    | "title"
                    | "role"
                    | "name"
                    | "type"
                    | "placeholder"
            ) || key.starts_with("aria-")
                && !value
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("javascript:")
        })
        .into_iter()
        .filter(|(_, v)| {
            !v.trim_start()
                .to_ascii_lowercase()
                .starts_with("javascript:")
        })
        .collect();
        let styles = fields(&styles, 128, |key, _| STYLE_PROPS.contains(&key));
        let rect = fields(&rect, 64, |key, _| {
            matches!(key, "x" | "y" | "width" | "height")
        });
        Self {
            url: safe_url(value(&raw, "url")),
            selector: clamp(value(&raw, "selector"), 512),
            element_path: clamp(value(&raw, "elementPath"), 512),
            full_path: clamp(value(&raw, "fullPath"), 1024),
            html_snippet: safe_snippet(&raw),
            nearby_text: clamp(value(&raw, "nearbyText"), 500),
            attributes,
            styles,
            rect,
        }
    }

    pub(crate) fn to_prompt(&self, png: Option<&Path>) -> String {
        self.to_prompt_with_source(png, None)
    }

    pub(crate) fn to_prompt_with_source(
        &self,
        png: Option<&Path>,
        source: Option<&super::source::SourceLoc>,
    ) -> String {
        let mut prompt = String::from("<selected-element>\n");
        if let Some(url) = &self.url {
            prompt.push_str(&format!("url: {url}\n"));
        }
        prompt.push_str(&format!(
            "selector: {}\nelementPath: {}\nfullPath: {}\n",
            self.selector, self.element_path, self.full_path
        ));
        if let Some(source) = source {
            if source.line == 0 {
                prompt.push_str(&format!("source: {}\n", source.file));
            } else {
                prompt.push_str(&format!(
                    "source: {}:{}:{}\n",
                    source.file, source.line, source.column
                ));
            }
        }
        for (label, entries) in [
            ("attributes", &self.attributes),
            ("styles", &self.styles),
            ("rect", &self.rect),
        ] {
            if !entries.is_empty() {
                prompt.push_str(&format!("{label}: "));
                prompt.push_str(
                    &entries
                        .iter()
                        .map(|(key, value)| format!("{key}={value}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                prompt.push('\n');
            }
        }
        prompt.push_str(&format!(
            "htmlSnippet: {}\nnearbyText: {}\n",
            self.html_snippet, self.nearby_text
        ));
        if let Some(path) = png {
            prompt.push_str(&format!("[image: {}]\n", path.display()));
        }
        prompt.push_str("</selected-element>\n");
        let prompt = crate::redaction::redact_secrets(&strip_controls(&prompt));
        clamp(&prompt, MAX_PROMPT_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hostile_page_data_is_bounded_and_cannot_write_terminal_controls() {
        let raw = json!({
            "url": "https://account:password@localhost:3000/app?token=secret#fragment",
            "selector": "button\u{1b}[201~evil",
            "elementPath": "main > button",
            "fullPath": "html > body > main > button",
            "nearbyText": "save\u{1b}[201~now",
            "htmlSnippet": "<button onclick='evil()'>".to_owned() + &"x".repeat(5_000_000) + "</button>",
            "attributes": {"onclick": "evil()", "href": "javascript:evil()", "aria-label": "token=0123456789abcdef0123456789abcdef0123456789abcdef"}
        });
        let payload = GrabPayload::from_raw(
            raw,
            json!({"color": "red"}),
            json!({"x": 1, "y": 2, "width": 3, "height": 4}),
        );
        let prompt = payload.to_prompt(None);
        assert!(prompt.len() <= 8192);
        assert!(!prompt.contains("onclick"));
        assert!(!prompt.contains("javascript:"));
        assert!(!prompt.contains("?token="));
        assert!(!prompt.contains("password@"));
        assert!(!prompt.contains("#fragment"));
        assert!(!prompt.contains('\u{1b}'));
        assert!(!prompt.contains("0123456789abcdef0123456789abcdef0123456789abcdef"));
        assert!(!prompt.contains("[image:"));
        assert!(prompt.contains("[REDACTED]"));
    }

    #[test]
    fn image_path_is_appended_only_when_capture_succeeds() {
        let payload = GrabPayload::from_raw(json!({"selector": "#save"}), json!({}), json!({}));
        assert!(!payload.to_prompt(None).contains("[image:"));
        assert!(
            payload
                .to_prompt(Some(std::path::Path::new("/tmp/grab.png")))
                .contains("[image: /tmp/grab.png]")
        );
    }

    #[test]
    fn source_location_is_included_in_grab_block() {
        let payload = GrabPayload::from_raw(json!({"selector": "#save"}), json!({}), json!({}));
        let source = crate::design_mode::source::SourceLoc {
            file: "src/App.tsx".into(),
            line: 12,
            column: 4,
        };
        assert!(
            payload
                .to_prompt_with_source(None, Some(&source))
                .contains("source: src/App.tsx:12:4")
        );
    }
}
