//! In-memory secrets. No raw-value getter, serialization, logging or persistence.
pub(crate) mod forms;
mod mask;
mod mobile;
pub(crate) mod policy;
mod run;
mod schema;
use schema::valid_name;
pub(crate) use schema::{Field, FieldKind, Form};
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use zeroize::{Zeroize, Zeroizing};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Submission {
    pub nonce: String,
    pub status: String,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    pub template: Option<Vec<String>>,
}
impl Drop for Submission {
    fn drop(&mut self) {
        self.nonce.zeroize();
        for value in self.values.values_mut() {
            value.zeroize();
        }
    }
}
#[cfg(test)]
impl Submission {
    fn stored(values: BTreeMap<String, String>) -> Self {
        Self {
            nonce: String::new(),
            status: "stored".into(),
            values,
            template: None,
        }
    }
    fn declined() -> Self {
        Self {
            nonce: String::new(),
            status: "declined".into(),
            values: BTreeMap::new(),
            template: None,
        }
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct Status {
    pub names: Vec<String>,
    pub status: String,
}

struct Pending {
    form: Form,
    response: Option<tokio::sync::oneshot::Sender<Status>>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.form.nonce.zeroize();
    }
}

struct ApprovedTemplate {
    argv: policy::Template,
    names: Vec<String>,
    cwd: String,
}

#[derive(Default)]
struct Inner {
    values: BTreeMap<String, Zeroizing<String>>,
    templates: Vec<ApprovedTemplate>,
    pending: Option<Pending>,
}

#[derive(Default)]
pub(crate) struct SecretStore {
    inner: parking_lot::Mutex<Inner>,
    // Window lifetime outlives the pending reply. Inspection stays blocked until
    // native destruction, including a submit/close race and failed destruction.
    pub(crate) tls: parking_lot::RwLock<Option<axum_server::tls_rustls::RustlsConfig>>,
    epoch: std::sync::atomic::AtomicU64,
    windows: parking_lot::Mutex<std::collections::BTreeSet<String>>,
}

impl SecretStore {
    pub(crate) fn inspection_epoch(&self) -> u64 {
        self.epoch.load(std::sync::atomic::Ordering::Acquire)
    }
    pub(crate) fn tools_blocked(&self) -> bool {
        self.inner.lock().pending.is_some() || !self.windows.lock().is_empty()
    }
    pub(crate) fn open(&self, form: Form) -> Result<Form, String> {
        let mut inner = self.inner.lock();
        if inner.pending.is_some() || !self.windows.lock().is_empty() {
            return Err("A secret form is already open".into());
        }
        self.epoch.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        inner.pending = Some(Pending {
            form: form.clone(),
            response: None,
        });
        Ok(form)
    }
    fn form(&self, nonce: &str) -> Result<Form, String> {
        let inner = self.inner.lock();
        inner
            .pending
            .as_ref()
            .filter(|p| nonce_equal(&p.form.nonce, nonce))
            .map(|p| p.form.clone())
            .ok_or_else(|| "Unknown or expired secret form".into())
    }
    pub(crate) fn submit(&self, nonce: &str, mut submission: Submission) -> Result<Status, String> {
        let mut inner = self.inner.lock();
        let form = &inner
            .pending
            .as_ref()
            .filter(|p| nonce_equal(&p.form.nonce, nonce))
            .ok_or("Unknown or expired secret form")?
            .form;
        let names: Vec<String> = form.fields.iter().map(|f| f.name.clone()).collect();
        let mut template = None;
        if submission.status == "declined" {
            if !submission.values.is_empty() || submission.template.is_some() {
                return Err("Decline must not contain values or templates".into());
            }
        } else if form.argv.is_some() {
            if submission.status != "approved" || !submission.values.is_empty() {
                return Err("Invalid command approval".into());
            }
            if let Some(argv) = submission.template.take() {
                let candidate = policy::Template::new(argv)?;
                if !candidate.matches(form.argv.as_ref().ok_or("Missing command")?) {
                    return Err("Template does not match this exact command".into());
                }
                template = Some(candidate);
            }
        } else {
            if submission.status != "stored" || submission.template.is_some() {
                return Err("Invalid secret submission".into());
            }
            let expected: Vec<&str> = form
                .fields
                .iter()
                .filter(|f| f.kind != FieldKind::Sso)
                .map(|f| f.name.as_str())
                .collect();
            if submission.values.len() != expected.len()
                || expected.iter().any(|name| {
                    !submission
                        .values
                        .get(*name)
                        .is_some_and(|v| !v.is_empty() && v.len() <= 16384 && !v.contains('\0'))
                })
            {
                return Err("Values must match the requested fields".into());
            }
        }
        // Validate the whole payload before consuming the nonce or mutating values.
        if submission.status == "stored" {
            for (name, value) in std::mem::take(&mut submission.values) {
                inner.values.insert(name, Zeroizing::new(value));
            }
        }
        if let Some(template) = template {
            let cwd = inner
                .pending
                .as_ref()
                .and_then(|p| p.form.cwd.clone())
                .ok_or("Missing approval directory")?;
            inner.templates.push(ApprovedTemplate {
                argv: template,
                names: names.clone(),
                cwd,
            });
        }
        let result = Status {
            names,
            status: submission.status.clone(),
        };
        if let Some(mut pending) = inner.pending.take()
            && let Some(tx) = pending.response.take()
        {
            let _ = tx.send(result.clone());
        }
        Ok(result)
    }
    pub(crate) fn mask(&self, text: &str) -> String {
        let inner = self.inner.lock();
        let needles: Vec<_> = inner
            .values
            .values()
            .flat_map(|v| mask::representations(v))
            .collect();
        mask::mask(text, &needles)
    }
    pub(crate) fn clear(&self) {
        let mut inner = self.inner.lock();
        inner.values.clear();
        inner.templates.clear();
        inner.pending.take();
    }
    fn allowed(&self, argv: &[String], names: &[String], cwd: &str) -> bool {
        self.inner
            .lock()
            .templates
            .iter()
            .any(|t| t.argv.matches(argv) && t.names == names && t.cwd == cwd)
    }
    pub(crate) fn remove(&self, names: &[String]) {
        let mut inner = self.inner.lock();
        for name in names {
            inner.values.remove(name);
        }
    }
    fn environment(&self, names: &[String]) -> Result<BTreeMap<String, Zeroizing<String>>, String> {
        let inner = self.inner.lock();
        names
            .iter()
            .map(|name| {
                inner
                    .values
                    .get(name)
                    .map(|value| (name.clone(), Zeroizing::new(value.to_string())))
                    .ok_or_else(|| "Requested secret is missing".into())
            })
            .collect()
    }
}

fn nonce_equal(expected: &str, actual: &str) -> bool {
    expected.len() == actual.len()
        && expected
            .bytes()
            .zip(actual.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

pub(crate) use forms::{form_http, submit_http};
pub(crate) use run::handle_secret;

pub(crate) fn tool_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "secret",
        "description": "Request a private sensitive-field form, then run a user-approved argv with named values in the child environment only. Values are never returned. An approved command can exfiltrate. request opens a separate native form; HTTP/mobile entry uses its private one-time capability link. run uses user-approved command templates or asks for exact argv consent; shell/interpreter evaluation and environment dumpers are forbidden. Templates fix program/subcommands (gh api supports placeholders; other programs use exact argv); a whole {arg} placeholder accepts one safe non-option argument. Output masks exact values and base64, hex and URL encodings, including line wraps. remove zeroizes stored values. Requests require a desktop host; the phone can submit the desktop-opened form.",
        "inputSchema": {"type": "object", "properties": {
            "action": {"type": "string", "enum": ["request", "run", "remove"]},
            "fields": {"type": "array", "items": {"type": "object", "properties": {
                "name": {"type": "string"}, "kind": {"type": "string", "enum": ["username", "password", "otp", "sso"]}, "display": {"type": "string", "description": "HTTP(S) SSO link to display without navigating"}
            }, "required": ["name", "kind"], "additionalProperties": false}},
            "reason": {"type": "string"}, "names": {"type": "array", "items": {"type": "string"}},
            "argv": {"type": "array", "items": {"type": "string"}}, "cwd": {"type": "string"}
        }, "required": ["action"], "additionalProperties": false}
    })
}
