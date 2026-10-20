//! Desktop verification policy, enabled only by the dedicated binary entry point.
//! Saved configuration must never re-enable machine-wide integrations here.
use std::sync::OnceLock;

static PORT: OnceLock<u16> = OnceLock::new();

pub(crate) fn active() -> bool {
    #[cfg(test)]
    if test_root().is_some() {
        return true;
    }
    PORT.get().is_some()
}

pub(crate) fn port() -> Option<u16> {
    PORT.get().copied()
}

#[cfg(any(feature = "desktop", test))]
fn validate(instance: &str, port: &str) -> Result<u16, String> {
    crate::app_instance::AppInstance::named(instance)?;
    let port = port.parse::<u16>().map_err(|_| "Invalid agent-test port")?;
    if port < 1024 || port == 9876 {
        return Err("agent-test requires a non-production port in 1024..=65535".into());
    }
    Ok(port)
}

#[cfg(feature = "desktop")]
pub(crate) fn enable() -> Result<(), String> {
    if !cfg!(target_os = "macos") || !cfg!(debug_assertions) || cfg!(feature = "dictation") {
        return Err("agent-test requires a macOS debug build without the dictation feature".into());
    }
    let instance = std::env::var("TUIC_APP_INSTANCE").unwrap_or_default();
    let port = std::env::var("TUIC_PORT").unwrap_or_default();
    PORT.set(validate(&instance, &port)?)
        .map_err(|_| "agent-test policy already selected".into())
}

#[cfg(any(feature = "desktop", test))]
pub(crate) fn apply_config(config: &mut crate::config::AppConfig, port: u16) {
    config.services.server.enabled = true;
    config.services.server.port = port;
    config.services.server.ipv6_enabled = false;
    config.services.relay.enabled = false;
    config.global_hotkey = None;
    config.auto_update_enabled = false;
    config.auto_update_plugins_enabled = false;
    config.prevent_sleep_when_busy = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_test_rejects_default_config_and_production_port() {
        // Bug caught: a direct binary launch silently uses production state.
        for (instance, port) in [
            ("", "19876"),
            ("default", "19876"),
            ("../escape", "19876"),
            ("agent-test", "9876"),
            ("agent-test", "0"),
            ("agent-test", "65536"),
        ] {
            assert!(validate(instance, port).is_err(), "{instance:?}:{port}");
        }
        assert_eq!(validate("agent-test", "19876"), Ok(19876));
    }

    #[test]
    fn agent_test_overrides_saved_machine_wide_services() {
        // Bug caught: a previously edited instance enables relay/update/hotkeys on boot.
        let mut config = crate::config::AppConfig::default();
        config.services.relay.enabled = true;
        config.global_hotkey = Some("CommandOrControl+Shift+T".into());
        config.auto_update_enabled = true;
        config.auto_update_plugins_enabled = true;
        config.prevent_sleep_when_busy = true;
        apply_config(&mut config, 19876);
        assert!(config.services.server.enabled);
        assert_eq!(config.services.server.port, 19876);
        assert!(!config.services.server.ipv6_enabled);
        assert!(!config.services.relay.enabled);
        assert_eq!(config.global_hotkey, None);
        assert!(!config.auto_update_enabled);
        assert!(!config.auto_update_plugins_enabled);
        assert!(!config.prevent_sleep_when_busy);
    }
}

// Test policy and path routing are thread-local: parallel tests never inherit them.
#[cfg(test)]
thread_local! {
    static TEST_ROOT: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn test_root() -> Option<std::path::PathBuf> {
    TEST_ROOT.with(|root| root.borrow().clone())
}

#[cfg(test)]
pub(crate) fn with_test_profile(root: &std::path::Path, test: impl FnOnce()) {
    struct Restore(Option<std::path::PathBuf>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_ROOT.with(|root| *root.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(TEST_ROOT.with(|slot| slot.replace(Some(root.to_path_buf()))));
    test();
}
