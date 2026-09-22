//! Per-repository inspect-mode lifecycle. The CDP adapter is deliberately behind
//! a small port so a browser disconnect and a closed PTY can be tested without
//! launching Chrome or writing to a real terminal.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use chromiumoxide::cdp::browser_protocol::{css, dom, overlay, page};
use chromiumoxide::cdp::js_protocol::{debugger, runtime};
use chromiumoxide::{Browser, Page};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::payload::GrabPayload;
use super::source::{self, ScriptMaps};

const MAX_PNG_BYTES: usize = 2 * 1024 * 1024;
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub(crate) type Prefill = Arc<dyn Fn(&str, &str) -> Result<(), String> + Send + Sync>;
pub(crate) type Notify = Arc<dyn Fn(&ModeStatus) + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModeStatus {
    pub(crate) repo_path: String,
    pub(crate) session_id: String,
    pub(crate) status: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl Rect {
    fn from_value(value: &Value) -> Option<Self> {
        let number = |key| value.get(key)?.as_f64().filter(|number| number.is_finite());
        let rect = Self {
            x: number("x")?,
            y: number("y")?,
            width: number("width")?,
            height: number("height")?,
        };
        // A page can report arbitrarily large CSS boxes. Keep screenshot
        // allocation bounded; the textual grab still succeeds without an image.
        (rect.width > 0.0
            && rect.height > 0.0
            && rect.width <= 4096.0
            && rect.height <= 4096.0
            && rect.width * rect.height <= 4_194_304.0)
            .then_some(rect)
    }
}

pub(crate) struct Pick {
    pub(crate) raw: Value,
    pub(crate) styles: Value,
    pub(crate) rect: Value,
    pub(crate) source: Option<source::SourceLoc>,
}

pub(crate) trait InspectBrowser: Send + Sync {
    fn arm(&self) -> BoxFuture<'_, Result<(), String>>;
    fn next_pick(&self) -> BoxFuture<'_, Option<Pick>>;
    fn capture(&self, rect: Rect) -> BoxFuture<'_, Result<Option<Vec<u8>>, String>>;
    fn close(&self) -> BoxFuture<'_, Result<(), String>>;
}

struct Mode {
    status: ModeStatus,
    browser: Option<Arc<dyn InspectBrowser>>,
    listener: Option<JoinHandle<()>>,
    generation: u64,
}

#[derive(Clone)]
pub(crate) struct DesignModeManager {
    modes: Arc<Mutex<HashMap<String, Mode>>>,
    grab_dir: PathBuf,
    prefill: Prefill,
    notify: Notify,
}

impl DesignModeManager {
    pub(crate) fn new(grab_dir: PathBuf, prefill: Prefill, notify: Notify) -> Self {
        Self {
            modes: Arc::new(Mutex::new(HashMap::new())),
            grab_dir,
            prefill,
            notify,
        }
    }

    pub(crate) async fn status(&self, repo_path: &str) -> Option<ModeStatus> {
        self.modes
            .lock()
            .await
            .get(repo_path)
            .map(|mode| mode.status.clone())
    }

    pub(crate) async fn statuses(&self) -> Vec<ModeStatus> {
        let mut statuses: Vec<_> = self
            .modes
            .lock()
            .await
            .values()
            .map(|mode| mode.status.clone())
            .collect();
        statuses.sort_by(|left, right| left.repo_path.cmp(&right.repo_path));
        statuses
    }

    /// The factory is called only when the repo has no usable browser. A second
    /// start on an armed repo simply changes the PTY bound to the same listener.
    pub(crate) async fn start<F, Fut>(
        &self,
        repo_path: String,
        session_id: String,
        make_browser: F,
    ) -> Result<ModeStatus, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Arc<dyn InspectBrowser>, String>>,
    {
        let mut modes = self.modes.lock().await;
        if let Some(mode) = modes.get_mut(&repo_path) {
            if mode.status.status == "armed" {
                mode.status.session_id = session_id;
                let status = mode.status.clone();
                drop(modes);
                (self.notify)(&status);
                return Ok(status);
            }
        }

        let browser = match modes.get(&repo_path).and_then(|mode| mode.browser.clone()) {
            Some(browser) => browser,
            None => make_browser().await?,
        };
        if let Err(error) = browser.arm().await {
            if let Some(mode) = modes.get_mut(&repo_path) {
                mode.browser = None;
            }
            let _ = browser.close().await;
            return Err(error);
        }
        let generation = modes.get(&repo_path).map_or(1, |mode| mode.generation + 1);
        let status = ModeStatus {
            repo_path: repo_path.clone(),
            session_id,
            status: "armed",
        };
        let manager = self.clone();
        let key = repo_path.clone();
        let listening_browser = browser.clone();
        let listener = tokio::spawn(async move {
            manager.listen(key, generation, listening_browser).await;
        });
        modes.insert(
            repo_path,
            Mode {
                status: status.clone(),
                browser: Some(browser),
                listener: Some(listener),
                generation,
            },
        );
        drop(modes);
        (self.notify)(&status);
        Ok(status)
    }

    async fn listen(&self, repo_path: String, generation: u64, browser: Arc<dyn InspectBrowser>) {
        loop {
            let Some(pick) = browser.next_pick().await else {
                self.disconnected(&repo_path, generation).await;
                break;
            };
            if let Err(error) = self
                .handle_pick(&repo_path, generation, &browser, pick)
                .await
            {
                tracing::warn!(repo_path, %error, "Design Mode pick was not delivered");
            }
            if !self.is_armed(&repo_path, generation).await {
                break;
            }
            if let Err(error) = browser.arm().await {
                tracing::warn!(repo_path, %error, "Design Mode could not re-arm inspect mode");
                self.disconnected(&repo_path, generation).await;
                break;
            }
        }
    }

    async fn is_armed(&self, repo_path: &str, generation: u64) -> bool {
        self.modes
            .lock()
            .await
            .get(repo_path)
            .is_some_and(|mode| mode.generation == generation && mode.status.status == "armed")
    }

    async fn handle_pick(
        &self,
        repo_path: &str,
        generation: u64,
        browser: &Arc<dyn InspectBrowser>,
        pick: Pick,
    ) -> Result<(), String> {
        if !self.is_armed(repo_path, generation).await {
            return Ok(());
        }
        let png = match Rect::from_value(&pick.rect) {
            Some(rect) => {
                tokio::time::timeout(std::time::Duration::from_millis(350), browser.capture(rect))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .flatten()
                    .and_then(|bytes| self.save_png(&bytes).ok().flatten())
            }
            None => None,
        };
        let prompt = GrabPayload::from_raw(pick.raw, pick.styles, pick.rect)
            .to_prompt_with_source(png.as_deref(), pick.source.as_ref());
        let modes = self.modes.lock().await;
        let Some(mode) = modes.get(repo_path) else {
            return Ok(());
        };
        if mode.generation != generation || mode.status.status != "armed" {
            return Ok(());
        }
        (self.prefill)(&mode.status.session_id, &prompt)
    }

    fn save_png(&self, bytes: &[u8]) -> Result<Option<PathBuf>, String> {
        if bytes.is_empty() || bytes.len() > MAX_PNG_BYTES {
            return Ok(None);
        }
        std::fs::create_dir_all(&self.grab_dir).map_err(|error| error.to_string())?;
        let path = self.grab_dir.join(format!("{}.png", uuid::Uuid::now_v7()));
        std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
        Ok(Some(path))
    }

    async fn disconnected(&self, repo_path: &str, generation: u64) {
        let mut modes = self.modes.lock().await;
        let Some(mode) = modes.get_mut(repo_path) else {
            return;
        };
        if mode.generation != generation || mode.status.status != "armed" {
            return;
        }
        mode.status.status = "stopped";
        mode.browser = None;
        let status = mode.status.clone();
        drop(modes);
        (self.notify)(&status);
    }

    /// Closing the terminal disarms the mode but deliberately leaves Chrome
    /// alive, so the user can keep browsing the page.
    pub(crate) async fn session_closed(&self, session_id: &str) {
        let mut modes = self.modes.lock().await;
        let mut changed = Vec::new();
        for mode in modes.values_mut() {
            if mode.status.session_id == session_id && mode.status.status == "armed" {
                mode.status.status = "stopped";
                mode.generation += 1;
                if let Some(listener) = mode.listener.take() {
                    listener.abort();
                }
                changed.push(mode.status.clone());
            }
        }
        drop(modes);
        for status in changed {
            (self.notify)(&status);
        }
    }

    pub(crate) async fn stop(&self, repo_path: &str) -> Result<Option<ModeStatus>, String> {
        let (browser, status, changed) = {
            let mut modes = self.modes.lock().await;
            let Some(mode) = modes.get_mut(repo_path) else {
                return Ok(None);
            };
            let changed = mode.status.status != "stopped";
            mode.status.status = "stopped";
            mode.generation += 1;
            if let Some(listener) = mode.listener.take() {
                listener.abort();
            }
            (mode.browser.take(), mode.status.clone(), changed)
        };
        if changed {
            (self.notify)(&status);
        }
        if let Some(browser) = browser {
            browser.close().await?;
        }
        Ok(Some(status))
    }

    pub(crate) async fn stop_all(&self) {
        let repos: Vec<String> = self.modes.lock().await.keys().cloned().collect();
        for repo in repos {
            if let Err(error) = self.stop(&repo).await {
                tracing::warn!(repo_path = repo, %error, "Design Mode browser could not close");
            }
        }
    }
}

/// The concrete headed Chrome adapter. A dedicated profile is reused on a
/// second start; the handler task is owned here and ends with the browser.
struct CdpInspectBrowser {
    browser: Mutex<Browser>,
    page: Page,
    picks: Mutex<chromiumoxide::listeners::EventStream<overlay::EventInspectNodeRequested>>,
    maps: Arc<Mutex<ScriptMaps>>,
    handler_task: JoinHandle<()>,
    map_task: JoinHandle<()>,
}

impl CdpInspectBrowser {
    async fn connect(
        repo_root: &Path,
        dev_server_url: Option<&str>,
    ) -> Result<Arc<dyn InspectBrowser>, String> {
        let (browser, mut handler) = super::browser::launch_or_attach(repo_root).await?;
        let handler_task = tokio::spawn(async move { while handler.next().await.is_some() {} });
        let pages = browser.pages().await.map_err(|error| error.to_string())?;
        let mut page = None;
        for candidate in pages {
            let url = candidate.url().await.ok().flatten().unwrap_or_default();
            if url.starts_with("http://") || url.starts_with("https://") {
                page = Some(candidate);
                break;
            }
        }
        let new_page = page.is_none();
        let page = match page {
            Some(page) => page,
            None => browser
                .new_page("about:blank")
                .await
                .map_err(|error| error.to_string())?,
        };
        let picks = page
            .event_listener::<overlay::EventInspectNodeRequested>()
            .await
            .map_err(|error| error.to_string())?;
        let mut scripts = page
            .event_listener::<debugger::EventScriptParsed>()
            .await
            .map_err(|error| error.to_string())?;
        page.execute(dom::EnableParams::default())
            .await
            .map_err(|error| error.to_string())?;
        page.execute(css::EnableParams::default())
            .await
            .map_err(|error| error.to_string())?;
        page.execute(overlay::EnableParams::default())
            .await
            .map_err(|error| error.to_string())?;
        page.execute(debugger::EnableParams::default())
            .await
            .map_err(|error| error.to_string())?;
        if new_page {
            if let Some(url) = dev_server_url {
                page.goto(url).await.map_err(|error| error.to_string())?;
            }
        }
        let maps = Arc::new(Mutex::new(ScriptMaps::default()));
        let task_maps = maps.clone();
        let map_task = tokio::spawn(async move {
            let permits = Arc::new(tokio::sync::Semaphore::new(4));
            let mut jobs = tokio::task::JoinSet::new();
            while let Some(event) = scripts.next().await {
                while let Some(result) = jobs.try_join_next() {
                    if let Err(error) = result {
                        tracing::warn!(%error, "Design Mode source-map task failed");
                    }
                }
                if let Some(map_url) = event.source_map_url.as_deref() {
                    let Ok(permit) = permits.clone().try_acquire_owned() else {
                        continue;
                    };
                    let maps = task_maps.clone();
                    let script_url = event.url.clone();
                    let map_url = map_url.to_owned();
                    jobs.spawn(async move {
                        let _permit = permit;
                        if let Some(bytes) = source::fetch_source_map(&script_url, &map_url).await {
                            maps.lock().await.insert(&script_url, &bytes);
                        }
                    });
                }
            }
        });
        Ok(Arc::new(Self {
            browser: Mutex::new(browser),
            page,
            picks: Mutex::new(picks),
            maps,
            handler_task,
            map_task,
        }))
    }

    async fn extract(&self, backend_node_id: dom::BackendNodeId) -> Result<Pick, String> {
        let node = self
            .page
            .execute(dom::ResolveNodeParams {
                backend_node_id: Some(backend_node_id),
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        let object_id = node
            .object
            .object_id
            .clone()
            .ok_or("DOM node has no runtime object")?;
        let call = self
            .page
            .execute(runtime::CallFunctionOnParams {
                function_declaration: include_str!("extract.js").to_owned(),
                object_id: Some(object_id),
                return_by_value: Some(true),
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        if call.exception_details.is_some() {
            return Err("DOM extraction failed".into());
        }
        let raw = call
            .result
            .result
            .value
            .ok_or("DOM extraction returned no value")?;
        let box_model = self
            .page
            .execute(dom::GetBoxModelParams {
                backend_node_id: Some(backend_node_id),
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        let points = box_model.model.border.inner();
        if points.len() != 8 {
            return Err("DOM box model has no border quad".into());
        }
        let xs = [points[0], points[2], points[4], points[6]];
        let ys = [points[1], points[3], points[5], points[7]];
        let x = xs.into_iter().fold(f64::INFINITY, f64::min);
        let y = ys.into_iter().fold(f64::INFINITY, f64::min);
        let width = xs.into_iter().fold(f64::NEG_INFINITY, f64::max) - x;
        let height = ys.into_iter().fold(f64::NEG_INFINITY, f64::max) - y;
        let rect = json!({ "x": x, "y": y, "width": width, "height": height });
        let mut maps = self.maps.lock().await;
        let source = source::resolve(&raw["source"], &mut maps);
        Ok(Pick {
            styles: raw["styles"].clone(),
            raw,
            rect,
            source,
        })
    }
}

impl InspectBrowser for CdpInspectBrowser {
    fn arm(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async {
            self.page
                .execute(overlay::SetInspectModeParams {
                    mode: overlay::InspectMode::SearchForNode,
                    highlight_config: Some(overlay::HighlightConfig {
                        show_info: Some(true),
                        ..Default::default()
                    }),
                })
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    fn next_pick(&self) -> BoxFuture<'_, Option<Pick>> {
        Box::pin(async {
            loop {
                let event = self.picks.lock().await.next().await?;
                match self.extract(event.backend_node_id).await {
                    Ok(pick) => return Some(pick),
                    Err(error) => {
                        tracing::warn!(%error, "Design Mode could not extract selected node")
                    }
                }
            }
        })
    }

    fn capture(&self, rect: Rect) -> BoxFuture<'_, Result<Option<Vec<u8>>, String>> {
        Box::pin(async move {
            let clip = page::Viewport {
                x: rect.x.max(0.0),
                y: rect.y.max(0.0),
                width: rect.width,
                height: rect.height,
                scale: 1.0,
            };
            let response = self
                .page
                .execute(page::CaptureScreenshotParams {
                    format: Some(page::CaptureScreenshotFormat::Png),
                    clip: Some(clip),
                    capture_beyond_viewport: Some(false),
                    ..Default::default()
                })
                .await
                .map_err(|error| error.to_string())?;
            let encoded: &str = response.data.as_ref();
            if encoded.len() > MAX_PNG_BYTES * 2 {
                return Ok(None);
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| error.to_string())?;
            Ok((bytes.len() <= MAX_PNG_BYTES).then_some(bytes))
        })
    }

    fn close(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async {
            self.map_task.abort();
            let mut browser = self.browser.lock().await;
            let graceful = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                browser.close().await.map_err(|error| error.to_string())?;
                browser.wait().await.map_err(|error| error.to_string())?;
                Ok::<(), String>(())
            })
            .await;
            if !matches!(&graceful, Ok(Ok(()))) {
                tracing::warn!(?graceful, "Design Mode Chrome needed forced shutdown");
                if let Some(result) = browser.kill().await {
                    result.map_err(|error| error.to_string())?;
                }
            }
            self.handler_task.abort();
            Ok(())
        })
    }
}

pub(crate) async fn live_browser(
    repo_root: &Path,
    dev_server_url: Option<&str>,
) -> Result<Arc<dyn InspectBrowser>, String> {
    CdpInspectBrowser::connect(repo_root, dev_server_url).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;

    struct FakeBrowser {
        picks: Mutex<mpsc::UnboundedReceiver<Pick>>,
        armed: AtomicUsize,
        closed: AtomicUsize,
        image: Option<Vec<u8>>,
        capture_delay: std::time::Duration,
    }

    impl InspectBrowser for FakeBrowser {
        fn arm(&self) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async {
                self.armed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
        fn next_pick(&self) -> BoxFuture<'_, Option<Pick>> {
            Box::pin(async { self.picks.lock().await.recv().await })
        }
        fn capture(&self, _rect: Rect) -> BoxFuture<'_, Result<Option<Vec<u8>>, String>> {
            Box::pin(async {
                tokio::time::sleep(self.capture_delay).await;
                Ok(self.image.clone())
            })
        }
        fn close(&self) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async {
                self.closed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    fn fake_with_delay(
        image: Option<Vec<u8>>,
        capture_delay: std::time::Duration,
    ) -> (Arc<FakeBrowser>, mpsc::UnboundedSender<Pick>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Arc::new(FakeBrowser {
                picks: Mutex::new(rx),
                armed: AtomicUsize::new(0),
                closed: AtomicUsize::new(0),
                image,
                capture_delay,
            }),
            tx,
        )
    }

    fn fake(image: Option<Vec<u8>>) -> (Arc<FakeBrowser>, mpsc::UnboundedSender<Pick>) {
        fake_with_delay(image, std::time::Duration::ZERO)
    }

    fn manager(
        dir: PathBuf,
    ) -> (
        DesignModeManager,
        mpsc::UnboundedReceiver<(String, String)>,
        mpsc::UnboundedReceiver<ModeStatus>,
    ) {
        let (prefill_tx, prefill_rx) = mpsc::unbounded_channel();
        let (notify_tx, notify_rx) = mpsc::unbounded_channel();
        let manager = DesignModeManager::new(
            dir,
            Arc::new(move |session, text| {
                prefill_tx
                    .send((session.to_owned(), text.to_owned()))
                    .map_err(|error| error.to_string())
            }),
            Arc::new(move |status| {
                let _ = notify_tx.send(status.clone());
            }),
        );
        (manager, prefill_rx, notify_rx)
    }

    fn pick() -> Pick {
        Pick {
            raw: json!({"selector":"#save", "tagName":"button", "textContent":"Save"}),
            styles: json!({"color":"red"}),
            rect: json!({"x":1.0,"y":2.0,"width":30.0,"height":20.0}),
            source: None,
        }
    }

    async fn recv<T>(rx: &mut mpsc::UnboundedReceiver<T>) -> T {
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn start_pick_prefills_once_and_rearms() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, mut events) = manager(dir.path().to_path_buf());
        let (browser, tx) = fake(Some(b"png-bytes".to_vec()));
        let status = manager
            .start("/repo".into(), "agent-1".into(), || async {
                Ok(browser.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        assert_eq!(status.status, "armed");
        assert_eq!(recv(&mut events).await.status, "armed");
        tx.send(pick()).unwrap();
        let (session, prompt) = recv(&mut prefilled).await;
        assert_eq!(session, "agent-1");
        assert!(prompt.contains("selector: #save"));
        assert!(prompt.contains("[image: "));
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while browser.armed.load(Ordering::SeqCst) < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            prefilled.try_recv().is_err(),
            "one pick must produce one prefill"
        );
        manager.stop_all().await;
    }

    #[tokio::test]
    async fn disconnect_stops_and_notifies() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, _, mut events) = manager(dir.path().to_path_buf());
        let (browser, tx) = fake(None);
        manager
            .start("/repo".into(), "agent".into(), || async {
                Ok(browser as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        assert_eq!(recv(&mut events).await.status, "armed");
        drop(tx);
        assert_eq!(recv(&mut events).await.status, "stopped");
        assert_eq!(manager.status("/repo").await.unwrap().status, "stopped");
    }

    #[tokio::test]
    async fn session_close_refuses_later_picks_and_keeps_chrome_open() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, mut events) = manager(dir.path().to_path_buf());
        let (browser, tx) = fake(None);
        manager
            .start("/repo".into(), "agent".into(), || async {
                Ok(browser.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        recv(&mut events).await;
        manager.session_closed("agent").await;
        assert_eq!(recv(&mut events).await.status, "stopped");
        tx.send(pick()).unwrap();
        tokio::task::yield_now().await;
        assert!(prefilled.try_recv().is_err());
        assert_eq!(browser.closed.load(Ordering::SeqCst), 0);
        manager.stop_all().await;
        assert_eq!(browser.closed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn second_start_rebinds_same_browser_and_stop_all_closes_each() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, _) = manager(dir.path().to_path_buf());
        let (browser_a, tx_a) = fake(None);
        let (browser_b, _tx_b) = fake(None);
        manager
            .start("/a".into(), "old".into(), || async {
                Ok(browser_a.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        manager
            .start("/a".into(), "new".into(), || async {
                panic!("active repo must not create another browser");
                #[allow(unreachable_code)]
                Ok(browser_a.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        manager
            .start("/b".into(), "other".into(), || async {
                Ok(browser_b.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        tx_a.send(pick()).unwrap();
        assert_eq!(recv(&mut prefilled).await.0, "new");
        manager.stop_all().await;
        assert_eq!(browser_a.closed.load(Ordering::SeqCst), 1);
        assert_eq!(browser_b.closed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stalled_capture_omits_image_but_still_prefills_and_rearms() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, _) = manager(dir.path().to_path_buf());
        let (browser, tx) = fake_with_delay(
            Some(b"png-bytes".to_vec()),
            std::time::Duration::from_secs(5),
        );
        manager
            .start("/repo".into(), "agent".into(), || async {
                Ok(browser.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        tx.send(pick()).unwrap();
        let (_, prompt) = recv(&mut prefilled).await;
        assert!(prompt.contains("selector: #save"));
        assert!(!prompt.contains("[image:"));
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while browser.armed.load(Ordering::SeqCst) < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        manager.stop_all().await;
    }

    #[test]
    fn invalid_rect_and_large_png_fail_closed() {
        assert!(Rect::from_value(&json!({"x":0,"y":0,"width":0,"height":10})).is_none());
        assert!(Rect::from_value(&json!({"x":0,"y":0,"width":1,"height":"NaN"})).is_none());
        assert!(Rect::from_value(&json!({"x":0,"y":0,"width":10000,"height":10000})).is_none());
        let dir = tempfile::tempdir().unwrap();
        let (manager, _, _) = manager(dir.path().to_path_buf());
        assert!(
            manager
                .save_png(&vec![0; MAX_PNG_BYTES + 1])
                .unwrap()
                .is_none()
        );
        assert!(!dir.path().read_dir().unwrap().any(|entry| entry.is_ok()));
    }
}
