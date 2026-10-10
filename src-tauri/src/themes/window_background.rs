use super::ThemeEntry;

/// Resolve the same selected-theme / Commander / hardcoded fallback as the UI.
pub(crate) fn resolve(themes: &[ThemeEntry], key: &str) -> [u8; 4] {
    let hex = themes
        .iter()
        .find(|theme| theme.key == key)
        .or_else(|| themes.iter().find(|theme| theme.key == "commander"))
        .map_or("#1e1e1e", |theme| theme.app_chrome.bg_primary.as_str());
    let (r, g, b) = super::parse_hex(hex);
    [r, g, b, 255]
}

#[cfg(feature = "desktop")]
pub(crate) fn current(app: &tauri::AppHandle) -> [u8; 4] {
    use tauri::Manager;
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let key = state.config.read().theme.clone();
    resolve(
        &super::load_themes(&crate::config::config_dir().join("themes")),
        &key,
    )
}

#[cfg(feature = "desktop")]
pub(crate) fn initialization_script([r, g, b, _]: [u8; 4]) -> String {
    // Root may not exist at document-start. Install the variable as soon as the
    // parser creates it; index.html's critical CSS consumes it before app boot.
    format!(
        r#"(() => {{
      const apply = () => {{
        if (!document.documentElement) return false;
        document.documentElement.style.setProperty('--bg-primary', '#{r:02x}{g:02x}{b:02x}');
        return true;
      }};
      if (!apply()) {{
        const observer = new MutationObserver(() => {{ if (apply()) observer.disconnect(); }});
        observer.observe(document, {{ childList: true }});
      }}
    }})();"#
    )
}

#[cfg(feature = "desktop")]
pub(crate) fn configure<'a>(
    builder: tauri::WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle>,
    color: [u8; 4],
) -> tauri::WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle> {
    builder
        .background_color(tauri::window::Color(color[0], color[1], color[2], color[3]))
        .initialization_script(initialization_script(color))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: startup chooses terminal color or a fixed dark color for light/custom app chrome.
    #[test]
    fn window_background_matches_every_bundled_theme_chrome() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let themes_dir = dir.path().join("themes");
        super::super::seed_builtin_themes(&themes_dir).unwrap();
        let themes = super::super::load_themes(&themes_dir);
        assert!(!themes.is_empty());
        for theme in &themes {
            let [r, g, b, a] = resolve(&themes, &theme.key);
            assert_eq!(
                format!("#{r:02x}{g:02x}{b:02x}"),
                theme.app_chrome.bg_primary.to_lowercase(),
                "{}",
                theme.key
            );
            assert_eq!(a, 255);
        }
    }

    // Catches: unknown saved keys use black instead of the UI's Commander fallback.
    #[test]
    fn window_background_unknown_theme_uses_commander_then_default() {
        assert_eq!(resolve(&[], "missing"), [30, 30, 30, 255]);
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let themes_dir = dir.path().join("themes");
        super::super::seed_builtin_themes(&themes_dir).unwrap();
        let themes = super::super::load_themes(&themes_dir);
        assert_eq!(resolve(&themes, "missing"), [30, 30, 30, 255]);
    }
}
