//! Runtime diagnostics — always-on CPU watchdog + toggleable diagnostic mode.
//!
//! ## Always on (zero-cost when idle)
//! - **CPU spike detection**: polls `getrusage(RUSAGE_SELF)` every 5s. PTY children
//!   (cargo, rustc, etc.) are separate OS processes and don't affect RUSAGE_SELF —
//!   this is intentional: the trigger watches TUIC's own runaway loops, not
//!   legitimate child load. Logs a diagnostic snapshot when CPU > 80% for 10+
//!   consecutive seconds; that snapshot lists per-child %cpu (`child_process_summary`).
//! - The periodic HEALTH log (diagnostic mode) also reports aggregate child CPU
//!   (`child_cpu_summary`) so a hot PTY child is visible even when TUIC itself is calm.
//!
//! ## Diagnostic mode (toggle at runtime)
//! When enabled via `set_diagnostic_mode(true)`, emits periodic health snapshots
//! covering failure patterns from past incidents:
//!
//! | Check                    | Past incident (mdkb)                     |
//! |--------------------------|------------------------------------------|
//! | CPU %                    | ack-flush-loop-cpu-spike                 |
//! | grid frames outstanding  | ui-freeze-investigation-2026-05-28       |
//! | Event bus throughput     | invoke.ts thundering herd comment         |
//! | Content index state      | content-index-global-semaphore           |
//! | FD / thread count trend  | (previous investigation session)         |
//! | Sleep/wake gap           | sleep-wake-false-idle-detection           |

use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const DIAGNOSTIC_POLL_INTERVAL: Duration = Duration::from_secs(10);
/// If the wall-clock gap between two consecutive watchdog polls exceeds this,
/// the machine was likely asleep (lid closed). Must stay well above
/// `DIAGNOSTIC_POLL_INTERVAL` (10s) so a normal slow tick never reads as sleep.
const SLEEP_WAKE_GAP: Duration = Duration::from_secs(30);
const CPU_THRESHOLD_PCT: f64 = 80.0;
const CONSECUTIVE_THRESHOLD: u32 = 2;
const STARTUP_DELAY: Duration = Duration::from_secs(30);
const COOLDOWN_BETWEEN_REPORTS: Duration = Duration::from_secs(60);

/// Once a broadcast receiver has fallen this far behind, a per-connection
/// consumer of a `tokio::sync::broadcast` channel should disconnect rather
/// than keep looping and re-lagging indefinitely — see
/// [`should_disconnect_for_lag`]. Shared by every such consumer (the two
/// per-session WS handlers in `mcp_http/session.rs`, and the global-bus SSE
/// handler in `mcp_http/sse_routes.rs`) so the threshold cannot drift between
/// them the way three independent ad hoc constants would.
///
/// Also the threshold this watchdog uses on its own `top_sessions_by_ws_lag`
/// attribution (see [`check_session_overload`]) — the same "this much lag is
/// clearly abnormal" judgment applies whether it is a single connection
/// deciding to disconnect or the watchdog deciding to name a session.
///
/// ~4x a single channel's capacity (256, `AppState::subscribe_pty_events`).
pub(crate) const MAX_CUMULATIVE_LAG: u64 = 1000;
/// Three `Lagged` in a row with no clean `Ok(event)` recv between them means
/// the receiver isn't merely behind, it's stuck.
pub(crate) const MAX_CONSECUTIVE_LAG: u32 = 3;

/// Decide whether a broadcast receiver that just lagged should be disconnected
/// rather than allowed to keep looping.
///
/// These are lifecycle/diff events with no cheap in-place resync — unlike the
/// grid *frame* `watch` channel, which just re-sends the current full frame on
/// a gap. Once a receiver is behind badly enough that it can't be trusted to
/// reflect current reality, the correct move is the one the client already
/// has to handle on any disconnect: close and reconnect, which re-fetches a
/// fresh snapshot before re-subscribing. Without this, a lagging receiver logs
/// a warning and loops forever — the exact shape of the `0b421c3a` incident,
/// where per-session WS lag climbed from 419ms to 12.4s with no self-correction.
///
/// `consecutive` resets to 0 on any clean recv; `cumulative` never resets for
/// the life of the connection — "is it stuck right now" vs. "how bad has this
/// connection's history been."
pub(crate) fn should_disconnect_for_lag(consecutive: u32, cumulative: u64) -> bool {
    consecutive >= MAX_CONSECUTIVE_LAG || cumulative >= MAX_CUMULATIVE_LAG
}

/// Per-tick event count above which a single session is independently flagged
/// as overloading the system, regardless of whether total process CPU ever
/// crosses `CPU_THRESHOLD_PCT`. This is the gap the `cddded98` incident
/// exposed: it drove sustained 100-200%+ process CPU for minutes via its own
/// screen/lifecycle repaint churn, but nothing named it as the cause until a
/// human manually correlated logs, `explain_state`, and `lsof` by hand.
/// Tunable — chosen as "clearly more than routine agent chatter," not measured
/// against production traffic.
const SESSION_EVENT_RATE_THRESHOLD: u64 = 500;
/// Per-tick output-byte count above which a single session is independently
/// flagged. 4 MiB in one 5-10s tick is far above a normal interactive agent's
/// output rate. Tunable, same caveat as `SESSION_EVENT_RATE_THRESHOLD`.
const SESSION_OUTPUT_BYTES_THRESHOLD: u64 = 4 * 1024 * 1024;
/// How many names `top_sessions_by_*` keeps — enough to see the top few
/// offenders without the CPU SPIKE line growing unbounded as session count does.
const TOP_N: usize = 5;
/// Minimum gap between two `SESSION OVERLOAD` reports for the *same*
/// `(session_id, axis)` pair — mirrors `COOLDOWN_BETWEEN_REPORTS`'s role for
/// `CPU SPIKE`, for the identical reason: a session sustaining overload for
/// several minutes (the `cddded98` incident's actual duration) would
/// otherwise log one near-duplicate line every tick (5-10s) the whole time.
/// Per-`(session, axis)` rather than global, so one loud session never
/// silences a report about a different session or a different axis.
const SESSION_OVERLOAD_COOLDOWN: Duration = Duration::from_secs(60);

/// FIXED (found in code review, 2026-09-29): `SESSION_EVENT_RATE_THRESHOLD`/
/// `SESSION_OUTPUT_BYTES_THRESHOLD` are expressed as counts "per nominal
/// `POLL_INTERVAL`" — but the tick length actually varies (`POLL_INTERVAL` 5s
/// vs. `DIAGNOSTIC_POLL_INTERVAL` 10s once diagnostic mode is on, or a tick
/// that simply ran long under load). Comparing a raw per-tick count straight
/// against these constants made the same sustained per-second rate take twice
/// as long to trip the trigger with diagnostic mode on — backwards from what
/// someone enabling it to investigate a live problem would expect.
/// `sessions_exceeding_thresholds` now normalizes each raw count to what it
/// would have been at a nominal-length tick before comparing, using the
/// tick's real elapsed wall-time (`run()` already computes this as
/// `wall_gap`, right before the sleep/wake check — no new timer needed).
/// Deliberately NOT applied to the `ws_lag` axis: lag isn't a steady
/// per-second production rate the way events/bytes are — a bad connection can
/// accumulate a large backlog in a fraction of a second, and that's exactly
/// as bad regardless of how long the tick containing it happened to be.
fn normalize_to_nominal_tick(raw: u64, elapsed: Duration) -> u64 {
    let elapsed_secs = elapsed.as_secs_f64();
    if elapsed_secs <= 0.0 {
        // Never expected in practice (`run()` only calls this after a real
        // `sleep(interval)`), but a raw count is a safe fallback over a
        // division by zero.
        return raw;
    }
    let nominal_secs = POLL_INTERVAL.as_secs_f64();
    ((raw as f64) * nominal_secs / elapsed_secs).round() as u64
}

/// Footprint at which the memory report is logged for the first time.
///
/// A healthy backend sits at a few hundred MB. 4 GB is an order of magnitude
/// above anything legitimate and still an order of magnitude below the ~30 GB
/// that got the process jetsammed on 2026-09-08 — so the report lands with
/// hours of headroom instead of after the app is already gone. That incident
/// left no evidence at all: by the time the footprint was visible, macOS marked
/// the process as not-debuggable and nothing could say which structure held the
/// memory. This is the line that makes the next one self-diagnosing.
const MEMORY_REPORT_FLOOR: u64 = 4 * 1024 * 1024 * 1024;

/// Global toggle — checked by the polling loop.
static DIAGNOSTIC_MODE: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_diagnostic_mode(on: bool) {
    let prev = DIAGNOSTIC_MODE.swap(on, Ordering::Relaxed);
    if prev != on {
        tracing::info!(
            source = "diagnostics",
            enabled = on,
            "Diagnostic mode {}",
            if on { "ENABLED" } else { "DISABLED" }
        );
    }
}

pub(crate) fn diagnostic_mode() -> bool {
    DIAGNOSTIC_MODE.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// CPU measurement via getrusage(RUSAGE_SELF)
// ---------------------------------------------------------------------------

// Never constructed on non-Unix (getrusage is POSIX-only) — the watchdog
// disables itself there. Suppress the resulting dead-code lint on Windows.
#[cfg_attr(not(unix), allow(dead_code))]
struct CpuSample {
    user_us: i64,
    sys_us: i64,
    wall: Instant,
}

impl CpuSample {
    #[cfg(unix)]
    fn now() -> Option<Self> {
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
        if ret != 0 {
            return None;
        }
        Some(Self {
            user_us: usage.ru_utime.tv_sec * 1_000_000 + usage.ru_utime.tv_usec as i64,
            sys_us: usage.ru_stime.tv_sec * 1_000_000 + usage.ru_stime.tv_usec as i64,
            wall: Instant::now(),
        })
    }

    /// `getrusage(RUSAGE_SELF)` is POSIX-only; there's no equivalent self-usage
    /// probe wired up on non-Unix, so CPU sampling is unavailable and the
    /// watchdog disables itself (callers treat `None` as "watchdog unavailable").
    #[cfg(not(unix))]
    fn now() -> Option<Self> {
        None
    }

    #[cfg_attr(not(unix), allow(dead_code))]
    fn cpu_pct_since(&self, prev: &CpuSample) -> f64 {
        let cpu_delta_us = (self.user_us - prev.user_us) + (self.sys_us - prev.sys_us);
        let wall_us = self.wall.duration_since(prev.wall).as_micros() as f64;
        if wall_us <= 0.0 {
            return 0.0;
        }
        (cpu_delta_us as f64 / wall_us) * 100.0
    }
}

// ---------------------------------------------------------------------------
// System probes (cheap, no allocations on the happy path)
// ---------------------------------------------------------------------------

fn count_open_fds() -> usize {
    #[cfg(target_os = "macos")]
    {
        std::fs::read_dir("/dev/fd").map_or(0, |d| d.count())
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir("/proc/self/fd").map_or(0, |d| d.count())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

fn thread_count() -> usize {
    #[cfg(target_os = "macos")]
    {
        let pid = std::process::id();
        std::process::Command::new("ps")
            .args(["-M", "-p", &pid.to_string()])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .count()
                    .saturating_sub(1)
            })
            .unwrap_or(0)
    }
    #[cfg(target_os = "linux")]
    {
        let pid = std::process::id();
        std::fs::read_dir(format!("/proc/{pid}/task")).map_or(0, |d| d.count())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

fn child_process_summary() -> String {
    let pid = std::process::id();
    // macOS `ps` doesn't support --ppid; use -o + awk to filter
    let output = std::process::Command::new("sh")
        .args([
            "-c",
            &format!("ps -eo pid,ppid,comm,%cpu | awk '$2 == {pid}'"),
        ])
        .output();
    match output {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                "(no children)".to_string()
            } else {
                s
            }
        }
        Err(_) => "(failed to list children)".to_string(),
    }
}

/// Compact aggregate %CPU of direct PTY children, for the periodic HEALTH log.
///
/// The spike trigger uses `getrusage(RUSAGE_SELF)`, which by design excludes
/// children (it watches TUIC's own runaway loops, not legitimate `cargo`/agent
/// load). So this is the only place child CPU surfaces during a diagnostic
/// session that ISN'T already a TUIC-process spike. `%cpu` from `ps` is the
/// process-lifetime average, not instantaneous — good enough for visibility.
fn child_cpu_summary() -> String {
    let pid = std::process::id();
    let output = std::process::Command::new("sh")
        .args(["-c", &format!("ps -eo ppid,comm,%cpu | awk '$1 == {pid}'")])
        .output();
    let Ok(o) = output else {
        return "children_cpu=(failed)".to_string();
    };
    let text = String::from_utf8_lossy(&o.stdout);
    let mut total = 0.0_f64;
    let mut top_comm = String::new();
    let mut top_pct = 0.0_f64;
    for line in text.lines() {
        // Columns: ppid comm %cpu. `comm` may contain spaces, so ppid is the
        // first token and %cpu the last; everything between is the name.
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 3 {
            continue;
        }
        let pct: f64 = tokens[tokens.len() - 1].parse().unwrap_or(0.0);
        total += pct;
        if pct > top_pct {
            top_pct = pct;
            top_comm = tokens[1..tokens.len() - 1].join(" ");
        }
    }
    if top_comm.is_empty() {
        "children_cpu=0.0%".to_string()
    } else {
        format!("children_cpu={total:.1}% (top: {top_comm} {top_pct:.1}%)")
    }
}

// ---------------------------------------------------------------------------
// Snapshot: emitted on CPU spike or periodic diagnostic
// ---------------------------------------------------------------------------

struct HealthSnapshot {
    cpu_pct: f64,
    threads: usize,
    open_fds: usize,
    pty_sessions: usize,
    index_building: Vec<String>,
    index_sem_permits: usize,
    in_flight_stuck: Vec<String>,
    event_bus_subscribers: usize,
    git_cache_ttl_fallbacks: u64,
    head_emits_suppressed: u64,
    /// Events queued on the lossless per-session state lane and not yet applied.
    /// The lane is unbounded by design, so this is the only warning of a backlog.
    state_lane_depth: usize,
}

fn collect_snapshot(state: &Arc<AppState>, cpu_pct: f64) -> HealthSnapshot {
    // A session appears here with the number of frames it was sent and has not
    // reported back: anything above zero means the WebView is behind.
    let in_flight_stuck: Vec<String> = state
        .grid
        .gates
        .iter()
        .filter_map(|entry| {
            let outstanding = entry.value().outstanding();
            (outstanding > 0).then(|| format!("{}×{}", entry.key(), outstanding))
        })
        .collect();

    HealthSnapshot {
        cpu_pct,
        threads: thread_count(),
        open_fds: count_open_fds(),
        pty_sessions: state.session_maps.sessions.len(),
        index_building: state.index_in_flight.iter().map(|r| r.clone()).collect(),
        index_sem_permits: state.index_build_sem.available_permits(),
        in_flight_stuck,
        event_bus_subscribers: state.event_bus.receiver_count(),
        git_cache_ttl_fallbacks: state.git_cache.ttl_fallbacks.load(Ordering::Relaxed),
        head_emits_suppressed: state.repo_head_emits_suppressed.load(Ordering::Relaxed),
        state_lane_depth: state.session_maps.session_state_events.depth(),
    }
}

/// One tick's worth of per-session counters, drained (read-and-reset) once
/// per tick from `state.session_maps`. A single drain feeds both
/// [`log_spike`] (when it fires this tick) and [`check_session_overload`]
/// (which runs every tick) — draining twice in the same tick would make
/// whichever ran second see zeros for numbers the first already consumed.
struct SessionRates {
    event_counts: Vec<(String, u64)>,
    output_bytes: Vec<(String, u64)>,
    ws_lag: Vec<(String, u64)>,
}

fn collect_session_rates(state: &Arc<AppState>) -> SessionRates {
    let rates = SessionRates {
        event_counts: crate::state::drain_counter_map(&state.session_maps.session_event_counts),
        output_bytes: crate::state::drain_counter_map(&state.session_maps.session_output_bytes),
        ws_lag: crate::state::drain_counter_map(&state.session_maps.session_ws_lag),
    };
    // Bound the maps: `remove_live_session_state` drops a session's keys, but a
    // reader thread's last chunk or a late `emit_pty_event` can bump (and so
    // re-create) a key after that. Prune AFTER the drain so a session's final
    // tick is still reported.
    for map in [
        &state.session_maps.session_event_counts,
        &state.session_maps.session_output_bytes,
        &state.session_maps.session_ws_lag,
    ] {
        map.retain(|session_id, _| state.session_maps.sessions.contains_key(session_id));
    }
    rates
}

/// The `n` highest-count entries, highest first. Pure and separately
/// unit-tested so the ranking logic doesn't need a live process to verify.
fn top_n(counts: &[(String, u64)], n: usize) -> Vec<(String, u64)> {
    let mut sorted = counts.to_vec();
    sorted.sort_by_key(|a| std::cmp::Reverse(a.1));
    sorted.truncate(n);
    sorted
}

/// One session's per-tick number crossed the hard threshold for `axis`.
/// `axis` names which of the three counters tripped, for the log line.
struct Overload {
    session_id: String,
    axis: &'static str,
    value: u64,
}

/// Pure decision logic behind [`check_session_overload`], split out so the
/// threshold behavior is unit-testable without a live process or a tracing
/// subscriber to capture log output against. `elapsed` is the real wall-time
/// this tick actually took (`run()`'s own `wall_gap`) — each raw count is
/// normalized to what it would have been at a nominal-length tick before
/// comparing, per `normalize_to_nominal_tick`'s doc comment. `ws_lag` is
/// deliberately compared against its raw, un-normalized value — see the same
/// doc comment for why.
fn sessions_exceeding_thresholds(rates: &SessionRates, elapsed: Duration) -> Vec<Overload> {
    let mut overloaded = Vec::new();
    for (session_id, n) in &rates.event_counts {
        let normalized = normalize_to_nominal_tick(*n, elapsed);
        if normalized >= SESSION_EVENT_RATE_THRESHOLD {
            overloaded.push(Overload {
                session_id: session_id.clone(),
                axis: "events_per_tick",
                value: normalized,
            });
        }
    }
    for (session_id, n) in &rates.output_bytes {
        let normalized = normalize_to_nominal_tick(*n, elapsed);
        if normalized >= SESSION_OUTPUT_BYTES_THRESHOLD {
            overloaded.push(Overload {
                session_id: session_id.clone(),
                axis: "output_bytes_per_tick",
                value: normalized,
            });
        }
    }
    for (session_id, n) in &rates.ws_lag {
        if *n >= MAX_CUMULATIVE_LAG {
            overloaded.push(Overload {
                session_id: session_id.clone(),
                axis: "ws_lag_per_tick",
                value: *n,
            });
        }
    }
    overloaded
}

/// Pure decision behind the cooldown, split out so it's unit-testable without
/// a tracing subscriber to capture `check_session_overload`'s log output
/// against. `None` (never reported) always reports.
fn overload_cooldown_elapsed(now: Instant, last_reported: Option<Instant>) -> bool {
    match last_reported {
        None => true,
        Some(last) => now.duration_since(last) >= SESSION_OVERLOAD_COOLDOWN,
    }
}

/// Independent of the process-wide CPU-spike trigger: names a session whose
/// own per-tick numbers crossed a hard threshold, even on a tick where total
/// process CPU never crossed `CPU_THRESHOLD_PCT`. Runs every tick — see
/// `run()` — because the `cddded98` incident's whole shape was several
/// *individually* unremarkable-looking ticks whose combined effect was a
/// sustained process-wide spike with no single tick naming the cause.
///
/// `last_reported`, owned by `run()`'s loop and threaded through by `&mut`,
/// rate-limits repeats per `(session_id, axis)` pair to
/// `SESSION_OVERLOAD_COOLDOWN` — without it, a session sustaining overload for
/// the several-minute duration the motivating incident actually had would log
/// one near-duplicate line every single tick for the whole time (a code
/// review caught this asymmetry with `CPU SPIKE`'s own cooldown).
fn check_session_overload(
    rates: &SessionRates,
    elapsed: Duration,
    last_reported: &mut std::collections::HashMap<(String, &'static str), Instant>,
) {
    let now = Instant::now();
    // Bound the cooldown map: an entry past its window suppresses nothing, so
    // drop it rather than keep one per (session, axis) that ever overloaded.
    last_reported.retain(|_, last| !overload_cooldown_elapsed(now, Some(*last)));
    for o in sessions_exceeding_thresholds(rates, elapsed) {
        let key = (o.session_id.clone(), o.axis);
        if !overload_cooldown_elapsed(now, last_reported.get(&key).copied()) {
            continue;
        }
        last_reported.insert(key, now);
        tracing::warn!(
            source = "diagnostics",
            session_id = %o.session_id,
            axis = o.axis,
            value = o.value,
            "SESSION OVERLOAD: {} crossed {} = {} in one tick",
            o.session_id,
            o.axis,
            o.value,
        );
    }
}

fn log_spike(state: &Arc<AppState>, cpu_pct: f64, rates: &SessionRates) {
    let s = collect_snapshot(state, cpu_pct);
    let children = child_process_summary();
    let top_events = top_n(&rates.event_counts, TOP_N);
    let top_bytes = top_n(&rates.output_bytes, TOP_N);
    let top_lag = top_n(&rates.ws_lag, TOP_N);

    tracing::warn!(
        source = "diagnostics",
        "CPU SPIKE {:.1}% | threads={} fds={} sessions={} \
         index_building={:?} sem_permits={} in_flight_stuck={:?} \
         bus_subs={} git_cache_ttl_fallbacks={} head_emits_suppressed={} \
         state_lane={} top_sessions_by_event_rate={:?} \
         top_sessions_by_output_bytes={:?} top_sessions_by_ws_lag={:?}\n  children: {}",
        s.cpu_pct,
        s.threads,
        s.open_fds,
        s.pty_sessions,
        s.index_building,
        s.index_sem_permits,
        s.in_flight_stuck,
        s.event_bus_subscribers,
        s.git_cache_ttl_fallbacks,
        s.head_emits_suppressed,
        s.state_lane_depth,
        top_events,
        top_bytes,
        top_lag,
        children,
    );
}

fn log_periodic(state: &Arc<AppState>, cpu_pct: f64) {
    let s = collect_snapshot(state, cpu_pct);

    let stuck_note = if s.in_flight_stuck.is_empty() {
        String::new()
    } else {
        format!(" ⚠ in_flight_stuck={:?}", s.in_flight_stuck)
    };
    // `cpu` is TUIC-self only (RUSAGE_SELF); `children_cpu` covers PTY children
    // (cargo/agents) which the spike trigger deliberately ignores.
    let children = child_cpu_summary();

    tracing::info!(
        source = "diagnostics",
        "HEALTH cpu={:.1}% {} threads={} fds={} sessions={} \
         index={:?} sem={} bus_subs={} git_cache_ttl_fallbacks={} head_emits_suppressed={} \
         state_lane={}{}",
        s.cpu_pct,
        children,
        s.threads,
        s.open_fds,
        s.pty_sessions,
        s.index_building,
        s.index_sem_permits,
        s.event_bus_subscribers,
        s.git_cache_ttl_fallbacks,
        s.head_emits_suppressed,
        s.state_lane_depth,
        stuck_note,
    );
}

/// Say once when the desktop WebView's main thread stops running, and once when
/// it comes back.
///
/// This is the line that was missing on 2026-09-08: the UI was white for five
/// hours and nothing in the logs named the frontend. Diagnosis had to be
/// reconstructed by hand from the *absence* of frontend log lines.
fn report_frontend_liveness(state: &Arc<AppState>) {
    use crate::frontend_liveness::{FREEZE_AFTER, Verdict};

    match state.frontend_liveness.poll(FREEZE_AFTER) {
        Verdict::Quiet => {}
        Verdict::Frozen { gap } => tracing::warn!(
            source = "diagnostics",
            silent_secs = gap.as_secs(),
            "Frontend unresponsive: no heartbeat for {}s — the WebView main thread is blocked or gone. \
             The backend and every PTY session are unaffected; recover with \
             POST /debug/reload_webview, or open the UI in a browser on this port.",
            gap.as_secs(),
        ),
        Verdict::Recovered => tracing::info!(
            source = "diagnostics",
            "Frontend responsive again — heartbeat resumed"
        ),
    }
}

// ---------------------------------------------------------------------------
// Memory tripwire
// ---------------------------------------------------------------------------

/// The threshold to arm next, or `None` while `footprint` is still under
/// `armed`.
///
/// Thresholds only ever double, so a process that keeps growing is reported at
/// 4, 8, 16 GB — each report a fresh reading of which structure grew between
/// them — while a process that sits just under one is never reported twice.
/// A footprint that jumps several thresholds at once arms above where it
/// landed, so the next report means real further growth.
///
/// The arming is monotonic: memory that falls back below the line does not
/// re-arm it. Re-arming would flap around the threshold, and the first report
/// already names the structure.
fn next_memory_threshold(footprint: u64, armed: u64) -> Option<u64> {
    if footprint < armed {
        return None;
    }
    let mut next = armed;
    while next <= footprint {
        let doubled = next.saturating_mul(2);
        if doubled == next {
            break;
        }
        next = doubled;
    }
    Some(next)
}

/// Log where the memory is, at a level that survives log filtering.
fn log_memory_report(state: &Arc<AppState>, footprint: u64) {
    const GB: f64 = (1024 * 1024 * 1024) as f64;
    let report = crate::memory_report::report(state);
    tracing::error!(
        source = "diagnostics",
        report = %report,
        "Memory footprint {:.2} GB — this is far above a healthy backend and is what \
         gets the app killed by macOS under memory pressure. The report lists every \
         structure that grows, biggest first; `accounted_bytes` well below the \
         footprint means the memory belongs to something outside AppState.",
        footprint as f64 / GB,
    );
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

pub(crate) fn spawn(state: Arc<AppState>) {
    std::thread::Builder::new()
        .name("diagnostics".into())
        .spawn(move || run(state))
        .expect("failed to spawn diagnostics thread");
}

fn run(state: Arc<AppState>) {
    std::thread::sleep(STARTUP_DELAY);
    tracing::debug!(source = "diagnostics", "Diagnostics thread started");

    let mut prev = match CpuSample::now() {
        Some(s) => s,
        None => {
            tracing::warn!(
                source = "diagnostics",
                "getrusage failed — watchdog disabled"
            );
            return;
        }
    };

    let mut consecutive_high: u32 = 0;
    let mut last_spike_report = Instant::now() - COOLDOWN_BETWEEN_REPORTS;
    let mut last_periodic_report = Instant::now();
    let mut last_poll_wall = Instant::now();
    // Per-`(session_id, axis)` cooldown for `SESSION OVERLOAD` — see
    // `SESSION_OVERLOAD_COOLDOWN`'s doc comment.
    let mut last_overload_reported: std::collections::HashMap<(String, &'static str), Instant> =
        std::collections::HashMap::new();

    // Trend tracking for FD / thread growth
    let mut baseline_fds: Option<usize> = None;
    let mut baseline_threads: Option<usize> = None;

    // Memory tripwire — always on, not gated behind diagnostic mode. The
    // incident it exists for took 13 hours to build up with nobody watching.
    let mut armed_memory = MEMORY_REPORT_FLOOR;

    loop {
        let interval = if diagnostic_mode() {
            DIAGNOSTIC_POLL_INTERVAL
        } else {
            POLL_INTERVAL
        };
        std::thread::sleep(interval);

        // Sleep/wake detection: if wall-clock gap is way larger than poll interval,
        // the machine was asleep. Skip this tick to avoid stale deltas.
        let wall_gap = last_poll_wall.elapsed();
        last_poll_wall = Instant::now();
        if wall_gap > SLEEP_WAKE_GAP {
            tracing::info!(
                source = "diagnostics",
                gap_secs = wall_gap.as_secs(),
                "Sleep/wake detected — skipping tick"
            );
            // Tell the frontend a wake just happened so it can suppress the
            // false-busy completion cascade: on wake, idle shells/agents get
            // nudged busy→idle and would otherwise fire spurious completion
            // notifications (purple "unseen" dot + sound) for work that never ran.
            #[cfg(feature = "desktop")]
            {
                use tauri::Emitter;
                if let Some(ref app) = *state.app_handle.read() {
                    let _ = app.emit("system-wake", wall_gap.as_secs());
                }
            }
            // The JS thread not having run while the machine was off is not a
            // freeze. Without this every wake reports one.
            state.frontend_liveness.rebaseline();
            prev = CpuSample::now().unwrap_or(prev);
            consecutive_high = 0;
            continue;
        }

        report_frontend_liveness(&state);

        let current = match CpuSample::now() {
            Some(s) => s,
            None => continue,
        };

        let pct = current.cpu_pct_since(&prev);
        prev = current;

        // --- Session overload attribution (always on) ---
        // One drain per tick, shared with `log_spike` below when it fires this
        // same tick — see `SessionRates`'s doc comment for why this must not
        // drain twice.
        let session_rates = collect_session_rates(&state);
        check_session_overload(&session_rates, wall_gap, &mut last_overload_reported);

        // --- CPU spike detection (always on) ---
        if pct >= CPU_THRESHOLD_PCT {
            consecutive_high += 1;
            if consecutive_high >= CONSECUTIVE_THRESHOLD
                && last_spike_report.elapsed() >= COOLDOWN_BETWEEN_REPORTS
            {
                log_spike(&state, pct, &session_rates);
                last_spike_report = Instant::now();
                consecutive_high = 0;
            }
        } else {
            if consecutive_high >= CONSECUTIVE_THRESHOLD {
                tracing::info!(
                    source = "diagnostics",
                    cpu_pct = format!("{pct:.1}"),
                    "CPU spike resolved — back to {pct:.1}%"
                );
            }
            consecutive_high = 0;
        }

        // --- Memory tripwire (always on) ---
        // One `proc_pid_rusage` call per tick; the report itself is only built
        // when a threshold trips.
        if let Some(footprint) = crate::memory_report::phys_footprint_bytes()
            && let Some(next) = next_memory_threshold(footprint, armed_memory)
        {
            log_memory_report(&state, footprint);
            armed_memory = next;
        }

        // --- Diagnostic mode: periodic health snapshots ---
        if diagnostic_mode() && last_periodic_report.elapsed() >= Duration::from_secs(30) {
            log_periodic(&state, pct);
            last_periodic_report = Instant::now();

            // FD / thread growth trend
            let fds = count_open_fds();
            let threads = thread_count();
            let base_fds = *baseline_fds.get_or_insert(fds);
            let base_threads = *baseline_threads.get_or_insert(threads);

            if fds > base_fds + 50 {
                tracing::warn!(
                    source = "diagnostics",
                    "FD growth: {} → {} (+{} since baseline)",
                    base_fds,
                    fds,
                    fds - base_fds,
                );
            }
            if threads > base_threads + 20 {
                tracing::warn!(
                    source = "diagnostics",
                    "Thread growth: {} → {} (+{} since baseline)",
                    base_threads,
                    threads,
                    threads - base_threads,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn a_healthy_footprint_never_trips() {
        // A few hundred MB is normal and must stay silent forever, or the one
        // line that matters drowns in daily noise.
        assert_eq!(
            next_memory_threshold(400 * 1024 * 1024, MEMORY_REPORT_FLOOR),
            None
        );
    }

    #[test]
    fn crossing_the_floor_arms_the_next_doubling() {
        assert_eq!(
            next_memory_threshold(5 * GB, MEMORY_REPORT_FLOOR),
            Some(8 * GB),
            "reporting again at 8 GB means real further growth, not the same 5 GB twice"
        );
    }

    #[test]
    fn a_footprint_that_jumps_several_thresholds_is_reported_once() {
        // The 2026-09-08 shape: nobody was watching while it grew, and the
        // first reading was already deep past the floor. It must report there
        // and then arm above it, not walk every threshold it skipped.
        let armed = next_memory_threshold(40 * GB, MEMORY_REPORT_FLOOR);
        assert_eq!(armed, Some(64 * GB));
        assert_eq!(
            next_memory_threshold(41 * GB, armed.unwrap()),
            None,
            "still growing slowly at 41 GB is the same incident, not a new one"
        );
    }

    #[test]
    fn memory_falling_back_does_not_re_arm() {
        let armed = next_memory_threshold(5 * GB, MEMORY_REPORT_FLOOR).unwrap();
        assert_eq!(
            next_memory_threshold(GB, armed),
            None,
            "a footprint below the armed line is silent — re-arming would flap"
        );
    }

    /// The state lane is unbounded on purpose — dropping a SET or a CLEAR strands
    /// clients in a state that never existed or never ended — so a backlog is invisible
    /// until it is a memory problem. This snapshot is the only place it surfaces.
    ///
    /// A bare test `AppState` has no accumulator task, so nothing drains the lane: the
    /// same shape as the wedged consumer the metric exists to reveal.
    #[test]
    fn snapshot_reports_the_state_lane_backlog() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        assert_eq!(
            collect_snapshot(&state, 0.0).state_lane_depth,
            0,
            "an idle lane is empty"
        );

        for _ in 0..3 {
            state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                session_id: "s1".to_string(),
                parsed: serde_json::json!({ "type": "choice-cleared" }).into(),
            });
        }

        assert_eq!(
            collect_snapshot(&state, 0.0).state_lane_depth,
            3,
            "every queued event must be counted while it waits to be applied"
        );
    }

    #[test]
    fn a_receiver_within_bounds_is_not_disconnected() {
        assert!(!should_disconnect_for_lag(0, 0));
        assert!(!should_disconnect_for_lag(MAX_CONSECUTIVE_LAG - 1, 0));
        assert!(!should_disconnect_for_lag(0, MAX_CUMULATIVE_LAG - 1));
    }

    #[test]
    fn three_consecutive_lags_disconnect_even_with_small_cumulative_lag() {
        // Three Lagged in a row with no clean recv between means the receiver
        // is stuck right now, regardless of how small each individual gap was.
        assert!(should_disconnect_for_lag(MAX_CONSECUTIVE_LAG, 3));
    }

    #[test]
    fn cumulative_lag_past_the_cap_disconnects_even_with_a_low_consecutive_count() {
        // This is the `0b421c3a` shape: never 3-in-a-row, but the total kept
        // climbing (419ms -> 3.5s -> ... -> 12.4s) with clean recvs between —
        // consecutive resets each time, so only cumulative catches it.
        assert!(should_disconnect_for_lag(1, MAX_CUMULATIVE_LAG));
    }

    #[test]
    fn top_n_sorts_descending_and_truncates() {
        let counts = vec![
            ("a".to_string(), 3),
            ("b".to_string(), 10),
            ("c".to_string(), 1),
            ("d".to_string(), 7),
        ];
        assert_eq!(
            top_n(&counts, 2),
            vec![("b".to_string(), 10), ("d".to_string(), 7)]
        );
    }

    #[test]
    fn top_n_never_returns_more_than_asked() {
        let counts = vec![("a".to_string(), 1), ("b".to_string(), 2)];
        assert_eq!(
            top_n(&counts, 5).len(),
            2,
            "asking for more than exists is not an error"
        );
    }

    #[test]
    fn a_session_under_every_threshold_is_not_flagged() {
        let rates = SessionRates {
            event_counts: vec![("s1".to_string(), SESSION_EVENT_RATE_THRESHOLD - 1)],
            output_bytes: vec![("s1".to_string(), SESSION_OUTPUT_BYTES_THRESHOLD - 1)],
            ws_lag: vec![("s1".to_string(), MAX_CUMULATIVE_LAG - 1)],
        };
        assert!(sessions_exceeding_thresholds(&rates, POLL_INTERVAL).is_empty());
    }

    #[test]
    fn a_session_over_any_single_threshold_is_flagged_on_that_axis_only() {
        let rates = SessionRates {
            event_counts: vec![("hot".to_string(), SESSION_EVENT_RATE_THRESHOLD)],
            output_bytes: vec![("quiet".to_string(), 10)],
            ws_lag: vec![],
        };
        let flagged = sessions_exceeding_thresholds(&rates, POLL_INTERVAL);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].session_id, "hot");
        assert_eq!(flagged[0].axis, "events_per_tick");
    }

    #[test]
    fn a_session_over_multiple_thresholds_is_flagged_once_per_axis() {
        // This is the "one session is the whole incident" shape — it should
        // be nameable on every axis it actually blew past, not just the first.
        let rates = SessionRates {
            event_counts: vec![("hot".to_string(), SESSION_EVENT_RATE_THRESHOLD)],
            output_bytes: vec![("hot".to_string(), SESSION_OUTPUT_BYTES_THRESHOLD)],
            ws_lag: vec![("hot".to_string(), MAX_CUMULATIVE_LAG)],
        };
        let flagged = sessions_exceeding_thresholds(&rates, POLL_INTERVAL);
        assert_eq!(flagged.len(), 3);
        assert!(flagged.iter().all(|o| o.session_id == "hot"));
    }

    #[test]
    fn normalize_to_nominal_tick_is_a_no_op_at_the_nominal_interval() {
        assert_eq!(normalize_to_nominal_tick(500, POLL_INTERVAL), 500);
    }

    #[test]
    fn normalize_to_nominal_tick_scales_down_a_longer_tick() {
        // Diagnostic mode's 10s tick is 2x POLL_INTERVAL (5s) — the same raw
        // count over twice the time is half the rate, so it must normalize
        // down to half, not pass through unchanged.
        assert_eq!(
            normalize_to_nominal_tick(500, DIAGNOSTIC_POLL_INTERVAL),
            250
        );
    }

    #[test]
    fn normalize_to_nominal_tick_scales_up_a_shorter_tick() {
        assert_eq!(normalize_to_nominal_tick(100, Duration::from_secs(1)), 500);
    }

    /// This is the exact bug a code review caught: without normalization, a
    /// session sustaining the SAME per-second rate that would trip the
    /// threshold at a normal 5s tick produces a raw count over
    /// `DIAGNOSTIC_POLL_INTERVAL` (10s, 2x longer) that's twice as large —
    /// and comparing that larger raw count straight against the same
    /// constant would (before this fix) have made the threshold effectively
    /// *harder* to hit per unit of real time while diagnostic mode is on,
    /// silently desensitizing the detector at the exact moment someone
    /// turned diagnostic mode on to look closer.
    #[test]
    fn a_rate_that_trips_the_threshold_at_a_normal_tick_still_trips_it_during_a_longer_diagnostic_tick()
     {
        // The same 100 events/sec rate that produces exactly the threshold
        // (500) over a normal 5s tick produces DOUBLE that raw count (1000)
        // over DIAGNOSTIC_POLL_INTERVAL's 10s — normalizing 1000 back down by
        // (5s/10s) gives exactly 500 again, which must still trip.
        let same_rate_raw_over_a_doubled_tick = SESSION_EVENT_RATE_THRESHOLD * 2;
        let rates = SessionRates {
            event_counts: vec![("hot".to_string(), same_rate_raw_over_a_doubled_tick)],
            output_bytes: vec![],
            ws_lag: vec![],
        };
        assert!(
            sessions_exceeding_thresholds(&rates, DIAGNOSTIC_POLL_INTERVAL)
                .iter()
                .any(|o| o.session_id == "hot"),
            "a sustained rate that would trip the threshold at a normal tick \
             must still trip it during diagnostic mode's longer tick"
        );
    }

    #[test]
    fn ws_lag_axis_is_never_normalized_by_tick_length() {
        // Lag isn't a steady per-second production rate — a backlog can build
        // in a fraction of a second and is exactly as bad regardless of how
        // long the tick containing it happened to be. A longer tick must NOT
        // shrink an already-over-threshold lag value below the threshold.
        let rates = SessionRates {
            event_counts: vec![],
            output_bytes: vec![],
            ws_lag: vec![("hot".to_string(), MAX_CUMULATIVE_LAG)],
        };
        let flagged = sessions_exceeding_thresholds(&rates, DIAGNOSTIC_POLL_INTERVAL);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].value, MAX_CUMULATIVE_LAG);
    }

    #[test]
    fn overload_cooldown_reports_the_first_time_with_no_prior_entry() {
        assert!(overload_cooldown_elapsed(Instant::now(), None));
    }

    #[test]
    fn overload_cooldown_suppresses_a_report_within_the_window() {
        let now = Instant::now();
        assert!(!overload_cooldown_elapsed(now, Some(now)));
    }

    #[test]
    fn overload_cooldown_reports_again_once_the_window_has_passed() {
        let last = Instant::now();
        let now = last + SESSION_OVERLOAD_COOLDOWN;
        assert!(
            overload_cooldown_elapsed(now, Some(last)),
            "exactly at the boundary must report — the incident this exists for \
             lasted several MINUTES, a one-tick-early re-report is harmless"
        );
    }

    /// End-to-end proof (not just the pure decision fn) that a sustained
    /// overload logs once, then stays silent for the cooldown window, rather
    /// than once per tick for the incident's whole multi-minute duration —
    /// the exact log-spam shape a code review caught missing entirely.
    #[test]
    fn check_session_overload_only_updates_last_reported_once_within_the_cooldown() {
        let rates = SessionRates {
            event_counts: vec![("hot".to_string(), SESSION_EVENT_RATE_THRESHOLD)],
            output_bytes: vec![],
            ws_lag: vec![],
        };
        let mut last_reported = std::collections::HashMap::new();
        check_session_overload(&rates, POLL_INTERVAL, &mut last_reported);
        let first_stamp = *last_reported
            .get(&("hot".to_string(), "events_per_tick"))
            .expect("first overload tick must record a timestamp");

        // A second tick, still overloaded, immediately after — must NOT
        // advance the stamp (proves the cooldown actually suppressed it,
        // not merely that the map entry happens to exist).
        check_session_overload(&rates, POLL_INTERVAL, &mut last_reported);
        assert_eq!(
            *last_reported
                .get(&("hot".to_string(), "events_per_tick"))
                .unwrap(),
            first_stamp,
            "a re-check within the cooldown window must not touch the timestamp"
        );
    }

    /// The watchdog drains `session_event_counts` once per tick — draining it
    /// a second time in the same tick (e.g. if a future change called
    /// `collect_session_rates` from two places) would make whichever call ran
    /// second see zeros for numbers the first already consumed. This pins the
    /// read-and-reset contract `drain_counter_map` promises.
    #[test]
    fn collecting_session_rates_drains_the_underlying_counters() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state
            .session_maps
            .session_event_counts
            .entry("s1".to_string())
            .or_default()
            .fetch_add(5, Ordering::Relaxed);

        let first = collect_session_rates(&state);
        assert_eq!(first.event_counts, vec![("s1".to_string(), 5)]);

        let second = collect_session_rates(&state);
        assert!(
            second.event_counts.is_empty(),
            "a second collection in the same tick must not double-count"
        );
    }

    /// The counter maps stay bounded by the live session set: a key re-created
    /// after its session was torn down (a reader thread's last chunk, a late
    /// event) is still reported on the next tick, then dropped.
    #[test]
    fn collecting_session_rates_prunes_keys_of_sessions_that_are_gone() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::state::tests_support::insert_dummy_session(&state, "live");
        crate::state::bump_counter(&state.session_maps.session_event_counts, "live", 3);
        crate::state::bump_counter(&state.session_maps.session_event_counts, "gone", 4);
        crate::state::bump_counter(&state.session_maps.session_output_bytes, "gone", 9);
        crate::state::bump_counter(&state.session_maps.session_ws_lag, "gone", 2);

        let rates = collect_session_rates(&state);
        let mut events = rates.event_counts.clone();
        events.sort();
        assert_eq!(
            events,
            vec![("gone".to_string(), 4), ("live".to_string(), 3)],
            "the tick that prunes a dead session still reports its last numbers"
        );
        assert!(state.session_maps.session_event_counts.contains_key("live"));
        for map in [
            &state.session_maps.session_event_counts,
            &state.session_maps.session_output_bytes,
            &state.session_maps.session_ws_lag,
        ] {
            assert!(
                !map.contains_key("gone"),
                "a key whose session is gone must not survive the tick"
            );
        }
    }

    #[test]
    fn the_overload_cooldown_map_drops_entries_past_their_window() {
        let rates = SessionRates {
            event_counts: vec![],
            output_bytes: vec![],
            ws_lag: vec![],
        };
        let mut last_reported = std::collections::HashMap::new();
        last_reported.insert(
            ("old".to_string(), "events_per_tick"),
            Instant::now() - SESSION_OVERLOAD_COOLDOWN - Duration::from_secs(1),
        );
        last_reported.insert(("recent".to_string(), "events_per_tick"), Instant::now());
        check_session_overload(&rates, POLL_INTERVAL, &mut last_reported);
        assert!(!last_reported.contains_key(&("old".to_string(), "events_per_tick")));
        assert!(last_reported.contains_key(&("recent".to_string(), "events_per_tick")));
    }
}
