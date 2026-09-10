//! Where the process's memory actually is.
//!
//! On 2026-09-08 the backend reached a **40.7 GB** physical footprint after 13
//! hours (`JetsamEvent-2026-09-08-214808` names it as the machine's largest
//! process; `vmmap` reports 1,453,700 live allocations in the default malloc
//! zone at 1% fragmentation, and `leaks` finds only 47 MB of it unreferenced).
//! So it is not fragmentation and not a classic leak: some structure the app
//! still holds a reference to grew and was never pruned.
//!
//! Nothing could say which one. Every candidate had to be excluded by reading
//! code and by re-running the workload on a second instance, and a 40 GB process
//! that macOS marks as not-debuggable cannot be asked what it is holding — the
//! evidence dies with the incident.
//!
//! This report is the answer to that. One request names the structure: entry
//! counts for every map that grows with sessions or clients, and measured bytes
//! for the four that hold the payloads. It is deliberately on-demand and not on
//! the diagnostics tick — walking a session's log lines is cheap once and is not
//! something to do every five seconds forever.

use crate::state::AppState;
use std::sync::Arc;

/// One growable structure, as reported.
///
/// `bytes` is `None` where a byte count would cost more than it is worth: for
/// those the entry count is the signal, because their values are small and
/// fixed-size. A structure that is big *per entry* always carries bytes.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct MapReport {
    pub(crate) name: &'static str,
    pub(crate) entries: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bytes: Option<usize>,
}

impl MapReport {
    fn counted(name: &'static str, entries: usize) -> Self {
        Self {
            name,
            entries,
            bytes: None,
        }
    }

    fn measured(name: &'static str, entries: usize, bytes: usize) -> Self {
        Self {
            name,
            entries,
            bytes: Some(bytes),
        }
    }
}

/// The process's physical footprint in bytes — resident *plus* compressed.
///
/// `ps` RSS is the wrong number and was misleading during the incident: it read
/// 0.52 GB while the process held 40 GB, because everything allocated and never
/// touched again ends up in the memory compressor. The compressed pages are what
/// the kernel charges against the machine, and what triggers the jetsam sweep.
#[cfg(target_os = "macos")]
pub(crate) fn phys_footprint_bytes() -> Option<u64> {
    // `ri_phys_footprint` is the same field `footprint(1)` prints. Read through
    // `proc_pid_rusage` rather than `task_info(TASK_VM_INFO)` because libc
    // exposes this one and the mach struct would mean a new dependency for a
    // single u64.
    let mut info: libc::rusage_info_v0 = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::proc_pid_rusage(
            std::process::id() as libc::c_int,
            libc::RUSAGE_INFO_V0,
            std::ptr::addr_of_mut!(info).cast(),
        )
    };
    (rc == 0).then_some(info.ri_phys_footprint)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn phys_footprint_bytes() -> Option<u64> {
    None
}

/// Live allocations in the default malloc zone, and the bytes they hold.
///
/// This is the number that separates "a structure in `AppState` grew" from
/// "the heap grew somewhere this report cannot see": when `footprint` climbs
/// and `accounted_bytes` does not, `blocks_in_use` says whether the memory is
/// in the Rust heap at all.
///
/// It is the same pair `vmmap` prints as ALLOCATION COUNT and BYTES ALLOCATED,
/// read from inside the process. `vmmap` itself is not an option: it suspends
/// the target for as long as it takes to walk every region — measured at **22
/// seconds** on this app, which is a visible UI freeze. `malloc_zone_statistics`
/// reads counters the allocator already maintains and returns immediately.
#[cfg(target_os = "macos")]
pub(crate) fn malloc_zone_stats() -> Option<(u64, u64)> {
    // Declared here rather than taken from `libc`, which exposes neither the
    // struct nor the two calls. Layout is `<malloc/malloc.h>`.
    #[repr(C)]
    #[derive(Default)]
    struct MallocStatistics {
        blocks_in_use: libc::c_uint,
        size_in_use: libc::size_t,
        max_size_in_use: libc::size_t,
        size_allocated: libc::size_t,
    }
    unsafe extern "C" {
        fn malloc_default_zone() -> *mut libc::c_void;
        fn malloc_zone_statistics(zone: *mut libc::c_void, stats: *mut MallocStatistics);
    }

    let mut stats = MallocStatistics::default();
    unsafe {
        let zone = malloc_default_zone();
        if zone.is_null() {
            return None;
        }
        malloc_zone_statistics(zone, &mut stats);
    }
    Some((stats.blocks_in_use as u64, stats.size_in_use as u64))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn malloc_zone_stats() -> Option<(u64, u64)> {
    None
}

/// Just the bytes half of [`malloc_zone_stats`], for callers weighing a
/// structure across its own construction.
pub(crate) fn malloc_bytes_in_use() -> Option<u64> {
    malloc_zone_stats().map(|(_, bytes)| bytes)
}

/// Every structure in `AppState` that grows with sessions, clients or repos.
///
/// Sorted by bytes and then by entries, so the culprit is the first row rather
/// than something to be found by reading all of them.
pub(crate) fn maps(state: &Arc<AppState>) -> Vec<MapReport> {
    let sm = &state.session_maps;
    let grid = &state.grid;

    // Measured: these four are the only ones whose *values* can be large.
    // The two halves of a vt log buffer are reported apart: the grid has a hard
    // ceiling (scrollback × columns × cell), the captured log lines do not,
    // because a span holds whatever the PTY printed. Summed, a runaway log is
    // indistinguishable from a terminal filling its scrollback.
    let (vt_log_bytes, vt_grid_bytes) = grid
        .vt_log_buffers
        .iter()
        .map(|e| {
            let buf = e.value().lock();
            (buf.log_bytes(), buf.grid_bytes())
        })
        .fold((0, 0), |(l, g), (dl, dg)| (l + dl, g + dg));
    let raw_ring_bytes: usize = grid
        .pty_raw_rings
        .iter()
        .map(|e| e.value().lock().len())
        .sum();
    let output_bytes: usize = sm
        .output_buffers
        .iter()
        .map(|e| e.value().lock().capacity())
        .sum();
    let inbox_bytes: usize = state
        .agent_inbox
        .iter()
        .map(|e| e.value().iter().map(|m| m.content.len()).sum::<usize>())
        .sum();
    // Measured, not counted: an index is the heaviest per-repo structure the
    // app holds and it is released only when the repo is retired, so a session
    // spent across many repos accumulates them. The count alone said nothing —
    // four indices and four hundred megabytes read the same.
    let index_bytes: usize = state
        .content_indices
        .iter()
        .map(|e| e.value().read().approx_bytes())
        .sum();

    let mut out = vec![
        MapReport::measured("grid.vt_log_lines", grid.vt_log_buffers.len(), vt_log_bytes),
        MapReport::measured(
            "grid.vt_log_grids",
            grid.vt_log_buffers.len(),
            vt_grid_bytes,
        ),
        MapReport::measured(
            "grid.pty_raw_rings",
            grid.pty_raw_rings.len(),
            raw_ring_bytes,
        ),
        MapReport::measured("output_buffers", sm.output_buffers.len(), output_bytes),
        MapReport::measured("agent_inbox", state.agent_inbox.len(), inbox_bytes),
        MapReport::counted("sessions", sm.sessions.len()),
        MapReport::counted("session_states", sm.session_states.len()),
        MapReport::counted("pty_event_channels", sm.pty_event_channels.len()),
        MapReport::counted("messaging_channels", sm.messaging_channels.len()),
        MapReport::counted("ws_clients", state.ws_clients.len()),
        MapReport::counted("session_html_tabs", sm.session_html_tabs.len()),
        MapReport::counted("input_buffers", sm.input_buffers.len()),
        MapReport::counted("grid.channels_watch", grid.watch.len()),
        MapReport::counted("grid.gates", grid.gates.len()),
        MapReport::measured("content_indices", state.content_indices.len(), index_bytes),
        MapReport::counted("repo_watchers", state.repo_watchers.len()),
        MapReport::counted("dir_watchers", state.dir_watchers.len()),
        MapReport::counted("mcp.sessions", state.mcp.sessions.len()),
        MapReport::counted("peer_agents", state.peer_agents.len()),
        MapReport::counted("pending_injections", state.pending_injections.len()),
        MapReport::counted("event_bus_subscribers", state.event_bus.receiver_count()),
    ];
    out.sort_by_key(|m| {
        (
            std::cmp::Reverse(m.bytes.unwrap_or(0)),
            std::cmp::Reverse(m.entries),
        )
    });
    out
}

/// The whole report, as the endpoint returns it.
pub(crate) fn report(state: &Arc<AppState>) -> serde_json::Value {
    let maps = maps(state);
    let accounted: usize = maps.iter().filter_map(|m| m.bytes).sum();
    let (blocks, heap_bytes) = malloc_zone_stats().unzip();
    serde_json::json!({
        "phys_footprint_bytes": phys_footprint_bytes(),
        // What the maps below explain. A footprint far above this is memory no
        // structure here owns — which is itself the finding, and says the next
        // suspect is outside `AppState`.
        "accounted_bytes": accounted,
        // The Rust heap as the allocator sees it. `heap_bytes` far above
        // `accounted_bytes` means the growth is in allocations no map here
        // owns; `blocks_in_use` rising with it says how many.
        "malloc_blocks_in_use": blocks,
        "malloc_bytes_in_use": heap_bytes,
        "maps": maps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_state_reports_every_map_at_zero() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let maps = maps(&state);
        assert!(
            maps.iter().all(|m| m.entries == 0),
            "a fresh AppState holds nothing: {maps:?}"
        );
    }

    #[test]
    fn the_biggest_structure_is_reported_first() {
        // The whole point of the ordering: during the incident the culprit had
        // to be found by reading candidates one by one. The row that matters
        // must be the first one.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.grid.pty_raw_rings.insert(
            "s1".into(),
            parking_lot::Mutex::new((0..4096).map(|_| 7u8).collect()),
        );
        let maps = maps(&state);
        assert_eq!(maps[0].name, "grid.pty_raw_rings");
        assert_eq!(maps[0].bytes, Some(4096));
    }

    #[test]
    fn the_heap_is_reported_as_live_blocks_and_the_bytes_they_hold() {
        // The discriminator: when the footprint climbs and `accounted_bytes`
        // stays flat, this pair says whether the memory is in the Rust heap at
        // all. A zero block count would silently answer "no" to every future
        // question, so the only useful assertion is that real counters arrive.
        let stats = malloc_zone_stats();
        #[cfg(target_os = "macos")]
        {
            let (blocks, bytes) = stats.expect("the default zone always exists");
            assert!(blocks > 0, "a running process holds live allocations");
            assert!(
                bytes >= blocks,
                "every live block holds at least a byte: {blocks} blocks, {bytes} bytes"
            );
        }
        #[cfg(not(target_os = "macos"))]
        assert_eq!(stats, None);
    }

    #[test]
    fn the_footprint_is_the_compressed_total_not_the_resident_one() {
        // `ps` RSS read 0.52 GB while the process held 40 GB. Anything that
        // reports less than the resident set would repeat that mistake, so the
        // only assertion worth making is that a real number comes back.
        let fp = phys_footprint_bytes();
        #[cfg(target_os = "macos")]
        assert!(
            fp.is_some_and(|b| b > 1024 * 1024),
            "a running process has a footprint above 1 MB, got {fp:?}"
        );
        #[cfg(not(target_os = "macos"))]
        assert_eq!(fp, None);
    }
}
