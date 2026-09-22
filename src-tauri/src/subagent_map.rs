//! Subagent execution map — reads the transcripts Claude Code writes for its
//! in-process subagents and turns them into a swimlane timeline.
//!
//! Claude records an `Agent` spawn in the *parent* transcript
//! (`<config>/projects/<cwd-slug>/<uuid>.jsonl`) but writes the subagent's own
//! turns to a separate file under `<uuid>/subagents/`. Measured over 582 real
//! transcripts: 833 spawns, and **zero** `isSidechain:true` rows in any parent
//! file. That is why a subagent is invisible in the terminal that spawned it,
//! and why the subagent file — not the parent — is the source of truth for a
//! lane.

use std::path::PathBuf;

use serde::Serialize;

/// One swimlane: a single subagent, described by its `agent-<id>.meta.json`.
///
/// Only `agentType`, `description` and `spawnDepth` appear in all 928 sampled
/// metas, so every other field is optional and its absence is ordinary. The
/// timing fields start empty and are filled from the subagent's own transcript —
/// the parent transcript cannot supply them (see the module comment).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct Lane {
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub agent_type: String,
    pub model: Option<String>,
    /// `in_process_teammate`, or absent for a blocking/async `Agent` call.
    /// Its absence is what predicts a `tool_use_id`, and vice versa.
    pub task_kind: Option<String>,
    pub color: Option<String>,
    pub spawn_depth: u32,
    /// Set when a subagent was itself spawned by another subagent.
    pub parent_agent_id: Option<String>,
    /// Exact join to the parent's `Agent` tool_use. Absent on teammates, which
    /// join by prompt containment instead.
    pub tool_use_id: Option<String>,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    pub running: bool,
}

impl Lane {
    /// Close the lane. The sole writer of the end state, so `running` and
    /// `ended_at_ms` cannot drift into disagreeing about the same transition.
    pub(crate) fn finish(&mut self, at_ms: i64) {
        self.ended_at_ms = Some(at_ms);
        self.running = false;
    }
}

/// Markers a lane can carry.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EventKind {
    /// Drawn on the *parent* lane, pointing at the child it started.
    Spawn,
    Tool,
    Text,
    Complete,
    /// Stands for the markers the cap dropped. Never silent.
    Overflow,
}

/// One marker on one lane.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct LaneEvent {
    pub lane: String,
    pub at_ms: i64,
    /// Relative to the payload's origin. Computed here so the page formats an
    /// offset rather than deriving one.
    pub offset_ms: i64,
    pub kind: EventKind,
    pub label: String,
    /// Consecutive identical tools collapse into one marker carrying the run
    /// length — what makes a 458-row lane legible rather than merely shorter.
    pub count: u32,
    /// Child lane id, on a `Spawn` marker only: the arrow's head.
    pub target: Option<String>,
}

/// A lane's markers plus the two timestamps only its own transcript can supply.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LaneTimeline {
    pub events: Vec<LaneEvent>,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
}

/// Markers kept per lane before the tail is summarised.
pub(crate) const MAX_EVENTS_PER_LANE: usize = 200;
/// Lanes kept per session.
pub(crate) const MAX_LANES: usize = 64;
/// A label is a tool name or a short state word; anything longer is a body that
/// has no business on the wire.
pub(crate) const MAX_LABEL_CHARS: usize = 64;
/// Cap on the two texts the join compares — a spawn's prompt and a lane's first
/// row. Neither ever leaves the process, but both mirror a file that can be tens
/// of megabytes, so they are bounded per item rather than left to follow it.
///
/// Truncation can only weaken the join, never break a lane: a shortened prompt
/// is still contained in the message that contained the whole one, and two
/// spawns sharing a 16 KB prefix would tie on `max_by_key` and draw one arrow at
/// the wrong child. No real prompt comes close.
const MAX_JOIN_TEXT_CHARS: usize = 16_384;

/// The lane standing for the terminal's own Claude session — where every
/// depth-0 spawn arrow starts. Not a subagent, so it has no transcript of its
/// own under `subagents/`.
pub(crate) const ROOT_LANE: &str = "main";

/// Parse one JSONL row into a marker.
///
/// `None` for a row that carries nothing to draw — a malformed line, a
/// timestamp that will not parse, or a message with no renderable block. A bad
/// line is skipped, never fatal: these files are read while Claude appends to
/// them, and one unparseable row must not cost the other 457.
pub(crate) fn parse_row(lane: &str, line: &str, origin_ms: i64) -> Option<LaneEvent> {
    let row: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let at_ms = row.get("timestamp").and_then(|t| t.as_str()).and_then(iso_to_ms)?;
    let content = row.get("message")?.get("content")?;

    // A tool call is the only thing worth a marker of its own; text rows mark
    // the turn boundaries that open and close the lane.
    let label_kind = match content {
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .find_map(|b| match b.get("type").and_then(|t| t.as_str()) {
                Some("tool_use") => Some((
                    b.get("name").and_then(|n| n.as_str()).unwrap_or("tool").to_owned(),
                    EventKind::Tool,
                )),
                _ => None,
            })
            .or(Some(("text".to_owned(), EventKind::Text))),
        serde_json::Value::String(_) => Some(("prompt".to_owned(), EventKind::Text)),
        _ => None,
    }?;

    Some(LaneEvent {
        lane: lane.to_owned(),
        at_ms,
        offset_ms: at_ms - origin_ms,
        kind: label_kind.1,
        label: truncate_label(&label_kind.0),
        count: 1,
        target: None,
    })
}

/// Collapse consecutive identical markers and cap the result.
///
/// Applied to the whole accumulated list rather than per read, so an incremental
/// tail (see `events_since`) cannot split a run across a chunk boundary and
/// report the same tool twice.
pub(crate) fn collapse_runs(events: Vec<LaneEvent>) -> Vec<LaneEvent> {
    let mut out: Vec<LaneEvent> = Vec::with_capacity(events.len());
    for e in events {
        match out.last_mut() {
            Some(prev) if prev.kind == e.kind && prev.label == e.label => {
                prev.count += 1;
                prev.at_ms = e.at_ms;
                prev.offset_ms = e.offset_ms;
            }
            _ => out.push(e),
        }
    }
    if out.len() > MAX_EVENTS_PER_LANE {
        let dropped = out.len() - (MAX_EVENTS_PER_LANE - 1);
        let tail = out.split_off(MAX_EVENTS_PER_LANE - 1);
        let last = tail.last().expect("split tail is non-empty");
        out.push(LaneEvent {
            lane: last.lane.clone(),
            at_ms: last.at_ms,
            offset_ms: last.offset_ms,
            kind: EventKind::Overflow,
            label: format!("+{dropped} more"),
            count: dropped as u32,
            target: None,
        });
    }
    out
}

/// Full timeline for one subagent transcript.
pub(crate) fn lane_timeline(lane: &str, jsonl: &str, origin_ms: i64) -> LaneTimeline {
    timeline_from_rows(
        jsonl
            .lines()
            .filter_map(|l| parse_row(lane, l, origin_ms))
            .collect(),
    )
}

/// The half of [`lane_timeline`] that works on already-parsed rows, so a cached
/// incremental read and a one-shot parse cannot disagree about where a lane
/// starts, ends, or finishes.
pub(crate) fn timeline_from_rows(raw: Vec<LaneEvent>) -> LaneTimeline {
    let started_at_ms = raw.first().map(|e| e.at_ms);
    let ended_at_ms = raw.last().map(|e| e.at_ms);
    let mut events = collapse_runs(raw);
    // The lane's own last row is the only evidence it finished: the parent's
    // tool_result acknowledges the spawn for 691 of 795 sampled calls and never
    // carries the report.
    if let Some(last) = events.last_mut()
        && last.kind == EventKind::Text
    {
        last.kind = EventKind::Complete;
        last.label = "complete".to_owned();
        last.count = 1;
    }
    LaneTimeline {
        events,
        started_at_ms,
        ended_at_ms,
    }
}

/// Per-file read cursor, so a 2s poll re-reads only what the agent appended.
///
/// Events are cached raw — parsed with origin 0, so `offset_ms == at_ms` — and
/// rebased at assembly time. The origin is the earliest spawn in the session and
/// moves when an earlier lane appears, which must not cost a re-read of every
/// transcript.
///
/// Parent transcripts get their own map. The biggest one on this machine is
/// 31 MB, so re-reading it for every poll would cost 15 MB/s of disk and JSON
/// parsing for a handful of new rows.
#[derive(Default)]
pub(crate) struct MapCache {
    lanes: std::collections::HashMap<PathBuf, LaneCursor>,
    parents: std::collections::HashMap<PathBuf, SpawnCursor>,
}

#[derive(Default)]
struct LaneCursor {
    offset: u64,
    events: Vec<LaneEvent>,
    /// The lane's first row, verbatim. The only text a teammate lane can be
    /// joined by, and it never changes once written.
    first_message: String,
}

#[derive(Default)]
struct SpawnCursor {
    offset: u64,
    spawns: Vec<ParentSpawn>,
}

/// What a cursor advance found.
struct Appended {
    /// Only whole lines. A partial trailing line stays unread.
    text: String,
    /// The file shrank, so everything parsed from it before is gone.
    restarted: bool,
}

/// Read the complete lines appended to `path` since `offset`, advancing it.
///
/// Shared by both cursors: the byte arithmetic is the part that must not be
/// written twice, because a second copy is a second chance to consume a
/// half-written row.
fn read_appended(path: &std::path::Path, offset: &mut u64) -> std::io::Result<Appended> {
    use std::io::{Read, Seek, SeekFrom};

    let len = std::fs::metadata(path)?.len();
    // Truncated or rotated. A stale offset would start reading from the middle
    // of a line, so the only safe answer is to start over.
    let restarted = len < *offset;
    if restarted {
        *offset = 0;
    }
    if len == *offset {
        return Ok(Appended {
            text: String::new(),
            restarted,
        });
    }

    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(*offset))?;
    let mut chunk = String::new();
    file.take(len - *offset).read_to_string(&mut chunk)?;

    // Stop at the last newline: anything after it is a row Claude is still
    // writing. Consuming it would parse garbage now and skip the real row when
    // it lands.
    let Some(end) = chunk.rfind('\n') else {
        return Ok(Appended {
            text: String::new(),
            restarted,
        });
    };
    chunk.truncate(end + 1);
    *offset += chunk.len() as u64;
    Ok(Appended {
        text: chunk,
        restarted,
    })
}

impl MapCache {
    /// Read whatever has been appended to a lane transcript since the last call.
    ///
    /// Returns how many rows were newly parsed — 0 when the file has not moved.
    pub(crate) fn ingest(&mut self, lane: &str, path: &std::path::Path) -> std::io::Result<usize> {
        let cursor = self.lanes.entry(path.to_path_buf()).or_default();
        let appended = read_appended(path, &mut cursor.offset)?;
        if appended.restarted {
            cursor.events.clear();
            cursor.first_message.clear();
        }
        if cursor.first_message.is_empty()
            && let Some(first) = appended.text.lines().next()
        {
            cursor.first_message = truncate_chars(first, MAX_JOIN_TEXT_CHARS);
        }
        let before = cursor.events.len();
        cursor
            .events
            .extend(appended.text.lines().filter_map(|line| parse_row(lane, line, 0)));
        Ok(cursor.events.len() - before)
    }

    /// Everything read from a lane transcript so far, still on the raw origin.
    pub(crate) fn events(&self, path: &std::path::Path) -> &[LaneEvent] {
        self.lanes
            .get(path)
            .map(|c| c.events.as_slice())
            .unwrap_or(&[])
    }

    /// The lane's first row, verbatim — what `join_spawns` matches a teammate by.
    pub(crate) fn first_message(&self, path: &std::path::Path) -> &str {
        self.lanes
            .get(path)
            .map(|c| c.first_message.as_str())
            .unwrap_or("")
    }

    /// Read the `Agent` calls appended to a parent transcript since the last call.
    pub(crate) fn ingest_spawns(&mut self, path: &std::path::Path) -> std::io::Result<usize> {
        let cursor = self.parents.entry(path.to_path_buf()).or_default();
        let appended = read_appended(path, &mut cursor.offset)?;
        if appended.restarted {
            cursor.spawns.clear();
        }
        let before = cursor.spawns.len();
        cursor
            .spawns
            .extend(appended.text.lines().filter_map(parse_parent_spawn));
        Ok(cursor.spawns.len() - before)
    }

    /// Every `Agent` call read from a parent transcript so far.
    pub(crate) fn spawns(&self, path: &std::path::Path) -> &[ParentSpawn] {
        self.parents
            .get(path)
            .map(|c| c.spawns.as_slice())
            .unwrap_or(&[])
    }
}

/// Move cached events onto a timeline origin. The single writer of `offset_ms`
/// outside `parse_row`, so the two cannot drift.
pub(crate) fn rebase_offsets(events: &mut [LaneEvent], origin_ms: i64) {
    for e in events {
        e.offset_ms = e.at_ms - origin_ms;
    }
}

fn iso_to_ms(ts: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| d.timestamp_millis())
}

fn truncate_label(label: &str) -> String {
    truncate_chars(label, MAX_LABEL_CHARS)
}

/// Cut to `max` characters, marking the cut. Counts characters rather than
/// bytes so a multi-byte label cannot be split mid-codepoint.
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    text.chars().take(max - 1).chain(['…']).collect()
}

/// An `Agent` spawn as recorded in the *parent* transcript.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParentSpawn {
    pub tool_use_id: String,
    pub at_ms: i64,
    /// The prompt handed to the subagent. For a teammate this is the only link
    /// back to its lane, because its meta carries no `toolUseId`.
    pub prompt: String,
}

/// Read one `Agent` call out of a parent transcript row.
///
/// `None` for every other row, which is nearly all of them: a parent transcript
/// is mostly other tools and their results. A `tool_result` carrying the same
/// `tool_use_id` is the *answer* to a spawn and must not read as a second one —
/// only a `tool_use` block named `Agent` counts.
pub(crate) fn parse_parent_spawn(line: &str) -> Option<ParentSpawn> {
    let row: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let at_ms = row.get("timestamp").and_then(|t| t.as_str()).and_then(iso_to_ms)?;
    let blocks = row.get("message")?.get("content")?.as_array()?;
    let call = blocks.iter().find(|b| {
        b.get("type").and_then(|t| t.as_str()) == Some("tool_use")
            && b.get("name").and_then(|n| n.as_str()) == Some("Agent")
    })?;
    Some(ParentSpawn {
        tool_use_id: call.get("id").and_then(|i| i.as_str())?.to_owned(),
        at_ms,
        prompt: call
            .get("input")
            .and_then(|i| i.get("prompt"))
            .and_then(|p| p.as_str())
            .map(|p| truncate_chars(p, MAX_JOIN_TEXT_CHARS))
            .unwrap_or_default(),
    })
}

/// The two fields of a lane the join needs, borrowed so the join stays pure.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LaneProbe<'a> {
    pub agent_id: &'a str,
    pub tool_use_id: Option<&'a str>,
    pub first_message: &'a str,
}

/// Map `agent_id` → parent `tool_use_id`.
///
/// Two disjoint mechanisms, verified against 928 real metas:
///
/// 1. `meta.json.toolUseId` — present on 541, and exact.
/// 2. prompt containment — the remaining 385 are `in_process_teammate`, whose
///    first user message wraps the parent's prompt verbatim.
///
/// The 2 that match neither are left unmapped on purpose. A lane renders from
/// its own transcript, so an absent mapping costs the spawn arrow and nothing
/// else; inventing a link would instead draw a confidently wrong one.
pub(crate) fn join_spawns(
    spawns: &[ParentSpawn],
    lanes: &[LaneProbe<'_>],
) -> std::collections::HashMap<String, String> {
    use std::collections::{HashMap, HashSet};
    let mut out: HashMap<String, String> = HashMap::new();
    let mut taken: HashSet<&str> = HashSet::new();

    // Exact first, and unconditionally: a lane naming a tool_use_id has already
    // answered the question, so it must never fall through to content matching
    // — not even when the id names no spawn we can see.
    for lane in lanes {
        let Some(id) = lane.tool_use_id else { continue };
        if spawns.iter().any(|s| s.tool_use_id == id) && taken.insert(id) {
            out.insert(lane.agent_id.to_owned(), id.to_owned());
        }
    }

    for lane in lanes.iter().filter(|l| l.tool_use_id.is_none()) {
        // Longest match wins. One prompt can be a prefix of another, and the
        // shorter one is then contained in the longer one's message too — first
        // match would hand the lane to the wrong spawn and strand the right one.
        let best = spawns
            .iter()
            .filter(|s| !s.prompt.is_empty() && !taken.contains(s.tool_use_id.as_str()))
            .filter(|s| lane.first_message.contains(&s.prompt))
            .max_by_key(|s| s.prompt.len());
        if let Some(s) = best {
            taken.insert(&s.tool_use_id);
            out.insert(lane.agent_id.to_owned(), s.tool_use_id.clone());
        }
    }
    out
}

/// Build a lane from the contents of an `agent-<id>.meta.json`.
///
/// Returns `None` only when the document is not a JSON object at all; a missing
/// optional field is never a failure.
pub(crate) fn parse_lane_meta(agent_id: &str, meta_json: &str) -> Option<Lane> {
    let meta: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(meta_json).ok()?;
    let string = |key: &str| {
        meta.get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let agent_type = string("agentType").unwrap_or_default();
    Some(Lane {
        // `name` is absent from 538 of 928 metas; an empty header card is worse
        // than a coarser label, so fall back rather than leave it blank.
        name: string("name")
            .or_else(|| (!agent_type.is_empty()).then(|| agent_type.clone()))
            .unwrap_or_else(|| agent_id.to_owned()),
        agent_id: agent_id.to_owned(),
        description: string("description").unwrap_or_default(),
        agent_type,
        model: string("model"),
        task_kind: string("taskKind"),
        color: string("color"),
        spawn_depth: meta
            .get("spawnDepth")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32,
        parent_agent_id: string("parentAgentId"),
        tool_use_id: string("toolUseId"),
        started_at_ms: None,
        ended_at_ms: None,
        running: true,
    })
}

/// Path to the directory holding one `.jsonl` + `.meta.json` pair per subagent
/// of `session_uuid`, without checking that it exists.
///
/// Built through `agent_session`, which honours a `CLAUDE_CONFIG_DIR` override.
/// `claude_usage::claude_projects_dir` looks like the same thing and is not: it
/// hardcodes `~/.claude`, so on a machine running `CLAUDE_CONFIG_DIR` it
/// silently resolves to a directory that holds none of the user's transcripts.
pub(crate) fn subagents_path(
    cwd: &str,
    config_dir: Option<&str>,
    session_uuid: &str,
) -> Option<PathBuf> {
    Some(
        crate::agent_session::claude_project_dir_path(cwd, config_dir)?
            .join(session_uuid)
            .join("subagents"),
    )
}

/// `subagents_path`, narrowed to a directory that is actually there.
///
/// A session with no subagents never gets the directory, so `None` is the
/// ordinary "this terminal spawned nothing" answer, not an error.
pub(crate) fn subagents_dir(
    cwd: &str,
    config_dir: Option<&str>,
    session_uuid: &str,
) -> Option<PathBuf> {
    let dir = subagents_path(cwd, config_dir, session_uuid)?;
    dir.is_dir().then_some(dir)
}

/// One session's map: every lane, every marker, and the instant the timeline
/// starts from.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub(crate) struct MapBody {
    pub origin_ms: i64,
    pub lanes: Vec<Lane>,
    pub events: Vec<LaneEvent>,
}

/// Build the map for one session from its subagents directory.
///
/// `parent_transcript` supplies spawn timing only. It is read through the same
/// cursor as the lanes, and its absence is not an error: a lane renders from its
/// own transcript whatever the join does, so a failed join costs the arrow's
/// timestamp and nothing else.
pub(crate) fn build_map(
    cache: &mut MapCache,
    subagents_dir: &std::path::Path,
    parent_transcript: &std::path::Path,
    root_name: &str,
) -> MapBody {
    let mut found = lane_files(subagents_dir);
    // By id first, so two lanes that started in the same millisecond — or that
    // have not started at all — keep a stable order between polls.
    found.sort_by(|a, b| a.0.cmp(&b.0));

    let mut lanes: Vec<Lane> = Vec::new();
    let mut per_lane: Vec<(Vec<LaneEvent>, String)> = Vec::new();
    for (agent_id, meta_path, jsonl_path) in found {
        let Ok(meta) = std::fs::read_to_string(&meta_path) else {
            continue;
        };
        let Some(mut lane) = parse_lane_meta(&agent_id, &meta) else {
            continue;
        };
        // A meta appears a moment before the transcript it describes, so a
        // missing or unreadable lane file is an empty lane, not a lost one.
        let _ = cache.ingest(&agent_id, &jsonl_path);
        let timeline = timeline_from_rows(cache.events(&jsonl_path).to_vec());
        lane.started_at_ms = timeline.started_at_ms;
        if timeline.events.last().map(|e| e.kind) == Some(EventKind::Complete)
            && let Some(end) = timeline.ended_at_ms
        {
            lane.finish(end);
        }
        lanes.push(lane);
        per_lane.push((timeline.events, cache.first_message(&jsonl_path).to_owned()));
    }

    if lanes.is_empty() {
        return MapBody::default();
    }

    // Oldest first, and capped: past MAX_LANES the diagram stops being readable
    // long before it stops being renderable.
    let mut order: Vec<usize> = (0..lanes.len()).collect();
    order.sort_by_key(|&i| lanes[i].started_at_ms.unwrap_or(i64::MAX));
    order.truncate(MAX_LANES);
    let lanes: Vec<Lane> = order.iter().map(|&i| lanes[i].clone()).collect();
    let per_lane: Vec<(Vec<LaneEvent>, String)> =
        order.iter().map(|&i| per_lane[i].clone()).collect();

    let _ = cache.ingest_spawns(parent_transcript);
    let spawns = cache.spawns(parent_transcript);
    let probes: Vec<LaneProbe<'_>> = lanes
        .iter()
        .zip(&per_lane)
        .map(|(lane, (_, first_message))| LaneProbe {
            agent_id: &lane.agent_id,
            tool_use_id: lane.tool_use_id.as_deref(),
            first_message,
        })
        .collect();
    let joined = join_spawns(spawns, &probes);

    let known: std::collections::HashSet<&str> =
        lanes.iter().map(|l| l.agent_id.as_str()).collect();
    let mut events: Vec<LaneEvent> = Vec::new();
    for (lane, (lane_events, _)) in lanes.iter().zip(&per_lane) {
        // The parent's own tool call is the better timestamp — the child's first
        // row lands after it. Without a join the lane times its own arrow.
        let at_ms = joined
            .get(&lane.agent_id)
            .and_then(|id| spawns.iter().find(|s| &s.tool_use_id == id))
            .map(|s| s.at_ms)
            .or(lane.started_at_ms);
        if let Some(at_ms) = at_ms {
            let source = lane
                .parent_agent_id
                .as_deref()
                .filter(|p| known.contains(p))
                .unwrap_or(ROOT_LANE);
            events.push(LaneEvent {
                lane: source.to_owned(),
                at_ms,
                offset_ms: at_ms,
                kind: EventKind::Spawn,
                label: truncate_label(&lane.name),
                count: 1,
                target: Some(lane.agent_id.clone()),
            });
        }
        events.extend(lane_events.iter().cloned());
    }

    let origin_ms = events.iter().map(|e| e.at_ms).min().unwrap_or(0);
    rebase_offsets(&mut events, origin_ms);
    events.sort_by_key(|e| e.at_ms);

    let mut all_lanes = vec![root_lane(root_name, origin_ms)];
    all_lanes.extend(lanes);
    MapBody {
        origin_ms,
        lanes: all_lanes,
        events,
    }
}

/// The terminal's own session as a lane. It has no meta on disk — it is not a
/// subagent — so it is built rather than parsed.
fn root_lane(name: &str, origin_ms: i64) -> Lane {
    Lane {
        agent_id: ROOT_LANE.to_owned(),
        name: truncate_label(name),
        description: String::new(),
        agent_type: "claude".to_owned(),
        model: None,
        task_kind: None,
        color: None,
        spawn_depth: 0,
        parent_agent_id: None,
        tool_use_id: None,
        started_at_ms: Some(origin_ms),
        ended_at_ms: None,
        running: true,
    }
}

/// Every `agent-<id>.meta.json` in a subagents dir, with the transcript beside
/// it. The row's own `agentId` is the file name minus the `agent-` prefix, which
/// is what `parentAgentId` on another lane points at.
fn lane_files(dir: &std::path::Path) -> Vec<(String, PathBuf, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".meta.json")?;
            let agent_id = stem.strip_prefix("agent-")?.to_owned();
            Some((
                agent_id,
                entry.path(),
                dir.join(format!("{stem}.jsonl")),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Component;

    fn fixture(name: &str) -> &'static str {
        match name {
            "fork" => include_str!("fixtures/subagent_map/fork.meta.json"),
            "teammate" => include_str!("fixtures/subagent_map/teammate.meta.json"),
            "minimal" => include_str!("fixtures/subagent_map/minimal.meta.json"),
            other => panic!("unknown fixture {other}"),
        }
    }

    /// A fork carries the exact parent link (`toolUseId`) and a nesting link
    /// (`parentAgentId`). Both must survive parsing — the first draws the spawn
    /// arrow, the second decides which lane the arrow starts from.
    #[test]
    fn subagent_map_lane_reads_a_fork_meta() {
        let lane = parse_lane_meta("acap-advisor-abc", fixture("fork")).expect("parses");
        assert_eq!(lane.agent_id, "acap-advisor-abc");
        assert_eq!(lane.name, "cap-advisor");
        assert_eq!(lane.agent_type, "fork");
        assert_eq!(lane.description, "Second opinion on the cap");
        assert_eq!(lane.model.as_deref(), Some("inherit"));
        assert_eq!(lane.tool_use_id.as_deref(), Some("toolu_FIXTURE_FORK"));
        assert_eq!(lane.parent_agent_id.as_deref(), Some("aroot-0000000000000001"));
        assert_eq!(lane.spawn_depth, 1);
        assert_eq!(lane.task_kind, None);
        assert!(lane.running, "a freshly parsed lane has not ended yet");
    }

    /// The teammate half of the join: no `toolUseId` at all. Parsing must not
    /// treat its absence as a failure — 385 of 928 real metas look like this.
    #[test]
    fn subagent_map_lane_reads_a_teammate_meta_without_a_tool_use_id() {
        let lane = parse_lane_meta("areviewer-x-def", fixture("teammate")).expect("parses");
        assert_eq!(lane.tool_use_id, None);
        assert_eq!(lane.task_kind.as_deref(), Some("in_process_teammate"));
        assert_eq!(lane.color.as_deref(), Some("blue"));
        assert_eq!(lane.name, "reviewer-x");
        assert_eq!(lane.spawn_depth, 0);
    }

    /// Only `agentType`, `description` and `spawnDepth` are present in all 928
    /// sampled metas. Everything else missing is the ordinary case.
    #[test]
    fn subagent_map_lane_tolerates_a_minimal_meta() {
        let lane = parse_lane_meta("ascan-123", fixture("minimal")).expect("parses");
        assert_eq!(lane.agent_type, "general-purpose");
        assert_eq!(lane.model, None);
        assert_eq!(lane.task_kind, None);
        assert_eq!(lane.color, None);
        assert_eq!(lane.tool_use_id, None);
        assert_eq!(lane.parent_agent_id, None);
    }

    /// `name` is absent from 538 of 928 metas, so the header card needs a chain
    /// of fallbacks rather than an empty title.
    #[test]
    fn subagent_map_lane_name_falls_back_to_agent_type_then_to_the_id() {
        let named = parse_lane_meta("aid-1", fixture("fork")).expect("parses");
        assert_eq!(named.name, "cap-advisor", "an explicit name wins");

        let unnamed = parse_lane_meta("aid-2", fixture("minimal")).expect("parses");
        assert_eq!(unnamed.name, "general-purpose", "falls back to agentType");

        let bare = parse_lane_meta("aid-3", r#"{"description":"d","spawnDepth":0}"#)
            .expect("parses");
        assert_eq!(bare.name, "aid-3", "falls back to the agent id");
    }

    #[test]
    fn subagent_map_lane_rejects_meta_that_is_not_an_object() {
        assert!(parse_lane_meta("aid", "not json").is_none());
        assert!(parse_lane_meta("aid", "[]").is_none());
    }

    fn row(ts: &str, tool: &str) -> String {
        format!(
            r#"{{"timestamp":"{ts}","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"x","name":"{tool}"}}]}}}}"#
        )
    }

    fn append(path: &std::path::Path, text: &str) {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open");
        f.write_all(text.as_bytes()).expect("write");
    }

    /// The whole point of the cursor: a 2s poll must not re-read a 1.2 MB
    /// transcript. The count of newly parsed rows is the only way to tell an
    /// incremental read from a full one that happens to return the same list.
    #[test]
    fn subagent_map_cursor_parses_only_appended_rows() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("agent-a.jsonl");
        append(&path, &format!("{}\n{}\n", row("2026-09-21T10:00:00Z", "Read"), row("2026-09-21T10:00:01Z", "Bash")));

        let mut cache = MapCache::default();
        assert_eq!(cache.ingest("a", &path).expect("read"), 2, "first read parses both rows");
        assert_eq!(cache.ingest("a", &path).expect("read"), 0, "nothing changed, nothing parsed");

        append(&path, &format!("{}\n", row("2026-09-21T10:00:02Z", "Write")));
        assert_eq!(cache.ingest("a", &path).expect("read"), 1, "only the appended row is parsed");
        assert_eq!(cache.events(&path).len(), 3, "the accumulated list still holds all three");
    }

    /// A poll can land while Claude is mid-write. Consuming the half-written
    /// line would parse garbage now and skip the real row later.
    #[test]
    fn subagent_map_cursor_leaves_a_partial_trailing_line_alone() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("agent-a.jsonl");
        let complete = row("2026-09-21T10:00:00Z", "Read");
        let partial = row("2026-09-21T10:00:01Z", "Bash");
        let (head, tail) = partial.split_at(partial.len() / 2);
        append(&path, &format!("{complete}\n{head}"));

        let mut cache = MapCache::default();
        assert_eq!(cache.ingest("a", &path).expect("read"), 1, "only the finished line");

        append(&path, &format!("{tail}\n"));
        assert_eq!(cache.ingest("a", &path).expect("read"), 1, "the completed line parses now");
        assert_eq!(cache.events(&path).len(), 2, "and exactly once — not twice");
    }

    /// A stale offset into a shorter file reads from the middle of a line.
    #[test]
    fn subagent_map_cursor_resets_when_the_file_shrinks() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("agent-a.jsonl");
        append(&path, &format!("{}\n{}\n{}\n", row("2026-09-21T10:00:00Z", "Read"), row("2026-09-21T10:00:01Z", "Bash"), row("2026-09-21T10:00:02Z", "Write")));

        let mut cache = MapCache::default();
        assert_eq!(cache.ingest("a", &path).expect("read"), 3);

        std::fs::write(&path, format!("{}\n", row("2026-09-21T11:00:00Z", "Glob"))).expect("truncate");
        assert_eq!(cache.ingest("a", &path).expect("read"), 1, "re-reads from zero");
        assert_eq!(cache.events(&path).len(), 1, "the stale events are dropped, not appended to");
    }

    #[test]
    fn subagent_map_cursor_keeps_lanes_apart_by_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let a = tmp.path().join("agent-a.jsonl");
        let b = tmp.path().join("agent-b.jsonl");
        append(&a, &format!("{}\n", row("2026-09-21T10:00:00Z", "Read")));
        append(&b, &format!("{}\n{}\n", row("2026-09-21T10:00:00Z", "Bash"), row("2026-09-21T10:00:01Z", "Bash")));

        let mut cache = MapCache::default();
        cache.ingest("a", &a).expect("read");
        cache.ingest("b", &b).expect("read");
        assert_eq!(cache.events(&a).len(), 1);
        assert_eq!(cache.events(&b).len(), 2);
        assert_eq!(cache.events(&a)[0].lane, "a");
        assert_eq!(cache.events(&b)[0].lane, "b");
    }

    /// The origin moves when an earlier lane shows up mid-session, so cached
    /// offsets have to be rebasable without re-reading the files.
    #[test]
    fn subagent_map_rebase_recomputes_offsets_against_a_new_origin() {
        let mut events = vec![
            parse_row("a", &row("2026-09-21T10:00:05Z", "Read"), 0).expect("row"),
        ];
        assert_eq!(events[0].offset_ms, events[0].at_ms, "cached raw, origin 0");
        let origin = events[0].at_ms - 5_000;
        rebase_offsets(&mut events, origin);
        assert_eq!(events[0].offset_ms, 5_000);
    }

    const LANE_JSONL: &str = include_str!("fixtures/subagent_map/lane.jsonl");
    /// 2026-09-21T10:00:00.000Z — the fixture's first row.
    const LANE_ORIGIN_MS: i64 = 1_789_984_800_000;

    fn labels(events: &[LaneEvent]) -> Vec<(String, u32)> {
        events.iter().map(|e| (e.label.clone(), e.count)).collect()
    }

    #[test]
    fn subagent_map_events_emit_one_marker_per_tool_use() {
        let t = lane_timeline("alane-fixture", LANE_JSONL, LANE_ORIGIN_MS);
        let tools: Vec<_> = t
            .events
            .iter()
            .filter(|e| e.kind == EventKind::Tool)
            .collect();
        assert_eq!(
            labels(&tools.into_iter().cloned().collect::<Vec<_>>()),
            vec![
                ("Read".to_string(), 1),
                ("Bash".to_string(), 3),
                ("Read".to_string(), 1),
            ],
            "three consecutive Bash rows collapse into one marker of 3, and the \
             Read either side stays separate"
        );
    }

    #[test]
    fn subagent_map_events_offsets_are_relative_to_the_origin() {
        let t = lane_timeline("alane-fixture", LANE_JSONL, LANE_ORIGIN_MS);
        let first = t.events.first().expect("at least one event");
        assert_eq!(first.offset_ms, 0, "the first row sits at the origin");
        assert_eq!(first.at_ms, LANE_ORIGIN_MS);
        let last = t.events.last().expect("at least one event");
        assert_eq!(last.offset_ms, 9_250, "last row is 9.25s after the origin");
    }

    /// The parent's `tool_result` is `{status: teammate_spawned}` for 358 of the
    /// 795 sampled spawns and never carries the report, so the end of a lane can
    /// only come from the lane's own last row.
    #[test]
    fn subagent_map_events_take_the_end_from_the_lanes_own_last_row() {
        let t = lane_timeline("alane-fixture", LANE_JSONL, LANE_ORIGIN_MS);
        assert_eq!(t.started_at_ms, Some(LANE_ORIGIN_MS));
        assert_eq!(t.ended_at_ms, Some(LANE_ORIGIN_MS + 9_250));
        assert_eq!(
            t.events.last().map(|e| e.kind),
            Some(EventKind::Complete),
            "the lane closes with a Complete marker"
        );
    }

    /// A 458-row transcript is ordinary. One marker per row is unreadable and
    /// unbounded, so the tail must be summarised rather than dropped silently.
    #[test]
    fn subagent_map_events_cap_a_long_lane_with_one_overflow_marker() {
        let mut rows = String::new();
        for i in 0..(MAX_EVENTS_PER_LANE + 50) {
            // Alternate so nothing collapses and the cap is what bites.
            let tool = if i % 2 == 0 { "Read" } else { "Bash" };
            rows.push_str(&format!(
                r#"{{"timestamp":"2026-09-21T10:00:00.000Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t{i}","name":"{tool}"}}]}}}}"#
            ));
            rows.push('\n');
        }
        let t = lane_timeline("a", &rows, LANE_ORIGIN_MS);
        assert_eq!(t.events.len(), MAX_EVENTS_PER_LANE);
        let last = t.events.last().expect("capped list is non-empty");
        assert_eq!(last.kind, EventKind::Overflow);
        assert!(
            last.label.contains("51"),
            "the overflow marker must name how many rows it stands for, got {:?}",
            last.label
        );
    }

    /// Prompts and tool results are the two places a secret handed to an agent
    /// would surface. Neither may reach the payload.
    #[test]
    fn subagent_map_events_never_carry_a_prompt_or_a_result_body() {
        let t = lane_timeline("alane-fixture", LANE_JSONL, LANE_ORIGIN_MS);
        for e in &t.events {
            assert!(
                !e.label.contains("Review the diff and report"),
                "a prompt body leaked into a label: {:?}",
                e.label
            );
            assert!(
                e.label.len() <= MAX_LABEL_CHARS,
                "label must be truncated, got {} chars",
                e.label.len()
            );
        }
    }

    #[test]
    fn subagent_map_events_skip_a_malformed_row_without_losing_the_rest() {
        let rows = format!("not json\n{{\"broken\":\n{LANE_JSONL}");
        let t = lane_timeline("alane-fixture", &rows, LANE_ORIGIN_MS);
        assert!(
            t.events.iter().any(|e| e.label == "Bash"),
            "a bad line must not abort the parse"
        );
    }

    fn spawn(id: &str, at_ms: i64, prompt: &str) -> ParentSpawn {
        ParentSpawn {
            tool_use_id: id.to_string(),
            at_ms,
            prompt: prompt.to_string(),
        }
    }

    /// 541 of 928 real metas carry `toolUseId`. Where it is present it is the
    /// whole answer and no content matching should run at all.
    #[test]
    fn subagent_map_join_uses_tool_use_id_when_present() {
        let spawns = [spawn("toolu_A", 10, "alpha"), spawn("toolu_B", 20, "beta")];
        let lanes = [LaneProbe {
            agent_id: "lane-1",
            tool_use_id: Some("toolu_B"),
            first_message: "nothing resembling either prompt",
        }];
        let joined = join_spawns(&spawns, &lanes);
        assert_eq!(joined.get("lane-1").map(String::as_str), Some("toolu_B"));
    }

    /// The other 385: no `toolUseId`, but the subagent's first message wraps the
    /// parent's prompt verbatim inside `<teammate-message …>`.
    #[test]
    fn subagent_map_join_matches_a_teammate_by_prompt_containment() {
        let spawns = [spawn("toolu_A", 10, "Harvest the calendar")];
        let lanes = [LaneProbe {
            agent_id: "lane-1",
            tool_use_id: None,
            first_message: "<teammate-message teammate_id=\"lead\" summary=\"x\">\nHarvest the calendar\n</teammate-message>",
        }];
        let joined = join_spawns(&spawns, &lanes);
        assert_eq!(joined.get("lane-1").map(String::as_str), Some("toolu_A"));
    }

    /// A fan-out of identically named reviewers is the case the name cannot
    /// resolve and the prompt can.
    #[test]
    fn subagent_map_join_separates_siblings_sharing_a_name() {
        let spawns = [
            spawn("toolu_A", 10, "Review src/auth.rs"),
            spawn("toolu_B", 11, "Review src/db.rs"),
        ];
        let lanes = [
            LaneProbe { agent_id: "lane-db", tool_use_id: None, first_message: "…Review src/db.rs…" },
            LaneProbe { agent_id: "lane-auth", tool_use_id: None, first_message: "…Review src/auth.rs…" },
        ];
        let joined = join_spawns(&spawns, &lanes);
        assert_eq!(joined.get("lane-db").map(String::as_str), Some("toolu_B"));
        assert_eq!(joined.get("lane-auth").map(String::as_str), Some("toolu_A"));
    }

    /// One prompt being a prefix of another is the trap: the shorter one matches
    /// the longer one's lane too. The most specific match has to win, or the
    /// first lane examined steals the wrong spawn and the real owner gets none.
    #[test]
    fn subagent_map_join_prefers_the_most_specific_prompt() {
        let spawns = [
            spawn("toolu_SHORT", 10, "Review"),
            spawn("toolu_LONG", 11, "Review the auth module"),
        ];
        let lanes = [LaneProbe {
            agent_id: "lane-1",
            tool_use_id: None,
            first_message: "<teammate-message>Review the auth module</teammate-message>",
        }];
        let joined = join_spawns(&spawns, &lanes);
        assert_eq!(joined.get("lane-1").map(String::as_str), Some("toolu_LONG"));
    }

    #[test]
    fn subagent_map_join_never_gives_one_spawn_to_two_lanes() {
        let spawns = [spawn("toolu_A", 10, "same prompt")];
        let lanes = [
            LaneProbe { agent_id: "lane-1", tool_use_id: None, first_message: "same prompt" },
            LaneProbe { agent_id: "lane-2", tool_use_id: None, first_message: "same prompt" },
        ];
        let joined = join_spawns(&spawns, &lanes);
        assert_eq!(joined.len(), 1, "one spawn, one lane: {joined:?}");
    }

    /// The failure that must stay harmless. 2 of 928 metas join to nothing; the
    /// caller still has to render their lanes, so the join reports absence
    /// rather than inventing a link.
    #[test]
    fn subagent_map_join_leaves_an_unmatched_lane_unmapped() {
        let spawns = [spawn("toolu_A", 10, "alpha")];
        let lanes = [
            LaneProbe { agent_id: "orphan", tool_use_id: None, first_message: "unrelated" },
            LaneProbe { agent_id: "ghost", tool_use_id: Some("toolu_GONE"), first_message: "" },
        ];
        let joined = join_spawns(&spawns, &lanes);
        assert!(joined.get("orphan").is_none());
        assert!(
            joined.get("ghost").is_none(),
            "a toolUseId naming no spawn must not fall through to content matching"
        );
    }

    /// An empty prompt would be contained in every message.
    #[test]
    fn subagent_map_join_ignores_an_empty_prompt() {
        let spawns = [spawn("toolu_EMPTY", 10, "")];
        let lanes = [LaneProbe { agent_id: "lane-1", tool_use_id: None, first_message: "anything" }];
        assert!(join_spawns(&spawns, &lanes).is_empty());
    }

    /// `running` and `ended_at_ms` must never disagree — one writer sets both.
    #[test]
    fn subagent_map_lane_finish_sets_the_end_and_clears_running() {
        let mut lane = parse_lane_meta("aid", fixture("minimal")).expect("parses");
        assert!(lane.running && lane.ended_at_ms.is_none());
        lane.finish(1_700_000_000_123);
        assert_eq!(lane.ended_at_ms, Some(1_700_000_000_123));
        assert!(!lane.running);
    }

    /// The slug, the uuid and `subagents` must arrive as three separate path
    /// components. Asserting the rendered string instead would pass on unix and
    /// fail on Windows for a path that is in fact correct — the separator is the
    /// host's business, the components are ours.
    fn tail_components(path: &PathBuf, n: usize) -> Vec<String> {
        let all: Vec<String> = path
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        all[all.len() - n..].to_vec()
    }

    #[test]
    fn subagent_map_path_uses_the_config_dir_override() {
        let path = subagents_path("/Users/foo/bar", Some("/tmp/cfg"), "uuid-1")
            .expect("override always resolves");
        assert_eq!(
            tail_components(&path, 4),
            vec!["projects", "-Users-foo-bar", "uuid-1", "subagents"],
        );
        assert!(
            path.starts_with("/tmp/cfg"),
            "must sit under the override, not ~/.claude: {}",
            path.display()
        );
    }

    #[test]
    fn subagent_map_path_slugs_a_unix_cwd() {
        let path = subagents_path("/Users/stefano.straus/Gits/p", Some("/c"), "u")
            .expect("resolves");
        assert_eq!(
            tail_components(&path, 3),
            vec!["-Users-stefano-straus-Gits-p", "u", "subagents"],
        );
    }

    /// A Windows cwd must slug the same way a Windows Claude does, and the
    /// result must still be assembled by `join` rather than by pasting a
    /// separator into a string.
    #[test]
    fn subagent_map_path_slugs_a_windows_cwd() {
        let path = subagents_path(r"C:\Users\foo\bar", Some("/c"), "u").expect("resolves");
        assert_eq!(
            tail_components(&path, 3),
            vec!["C:-Users-foo-bar", "u", "subagents"],
        );
    }

    #[test]
    fn subagent_map_dir_is_none_when_the_directory_is_absent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = tmp.path().to_string_lossy().into_owned();
        assert!(
            subagents_dir("/Users/foo/bar", Some(&cfg), "uuid-1").is_none(),
            "a session that spawned nothing has no subagents dir"
        );
    }

    #[test]
    fn subagent_map_dir_is_some_once_the_directory_exists() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = tmp.path().to_string_lossy().into_owned();
        let dir = subagents_path("/Users/foo/bar", Some(&cfg), "uuid-1").expect("resolves");
        std::fs::create_dir_all(&dir).expect("create");
        assert_eq!(subagents_dir("/Users/foo/bar", Some(&cfg), "uuid-1"), Some(dir));
    }

    const PARENT_JSONL: &str = include_str!("fixtures/subagent_map/parent.jsonl");

    /// A subagents dir holding one lane, beside the parent transcript that
    /// spawned it. Returns (subagents dir, parent transcript).
    fn session_tree(tmp: &std::path::Path, parent: &str) -> (PathBuf, PathBuf) {
        let dir = tmp.join("subagents");
        std::fs::create_dir_all(&dir).expect("create the subagents dir");
        std::fs::write(dir.join("agent-alane-fixture.meta.json"), fixture("teammate"))
            .expect("write the meta");
        std::fs::write(dir.join("agent-alane-fixture.jsonl"), LANE_JSONL)
            .expect("write the lane transcript");
        let transcript = tmp.join("parent.jsonl");
        std::fs::write(&transcript, parent).expect("write the parent transcript");
        (dir, transcript)
    }

    /// Only the `Agent` tool calls are spawns. A parent transcript is mostly
    /// other tools, and a `tool_result` naming the same id is the *answer* to a
    /// spawn, not a second one.
    #[test]
    fn subagent_map_parent_spawns_read_only_the_agent_tool_calls() {
        let spawns: Vec<ParentSpawn> = PARENT_JSONL
            .lines()
            .filter_map(parse_parent_spawn)
            .collect();
        assert_eq!(spawns.len(), 1, "one Agent call in the fixture");
        assert_eq!(spawns[0].tool_use_id, "toolu_FIXTURE_TEAMMATE");
        assert_eq!(spawns[0].prompt, "Review the diff and report");
        assert_eq!(spawns[0].at_ms, LANE_ORIGIN_MS - 2_000);
    }

    /// The terminal's own session is a lane too — it is where every depth-0
    /// spawn arrow starts, and Boss's reference diagram draws it as E0.
    #[test]
    fn subagent_map_build_puts_the_root_lane_first() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), PARENT_JSONL);
        let mut cache = MapCache::default();
        let body = build_map(&mut cache, &dir, &parent, "claude — tuicommander");

        assert_eq!(body.lanes.len(), 2, "the root lane plus the one subagent");
        assert_eq!(body.lanes[0].agent_id, ROOT_LANE);
        assert_eq!(body.lanes[0].name, "claude — tuicommander");
        assert_eq!(body.lanes[1].agent_id, "alane-fixture");
        assert_eq!(body.lanes[1].name, "reviewer-x");
    }

    /// The spawn marker belongs to the lane that *started* the subagent, and
    /// the parent transcript times it: the child's own first row lands after
    /// the tool call that created it.
    #[test]
    fn subagent_map_build_times_the_spawn_from_the_parent_transcript() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), PARENT_JSONL);
        let mut cache = MapCache::default();
        let body = build_map(&mut cache, &dir, &parent, "root");

        let spawn = body
            .events
            .iter()
            .find(|e| e.kind == EventKind::Spawn)
            .expect("the lane was spawned by something");
        assert_eq!(spawn.lane, ROOT_LANE, "the arrow starts at the parent");
        assert_eq!(spawn.target.as_deref(), Some("alane-fixture"));
        assert_eq!(spawn.at_ms, LANE_ORIGIN_MS - 2_000);
        assert_eq!(body.origin_ms, LANE_ORIGIN_MS - 2_000);
        assert_eq!(spawn.offset_ms, 0, "the earliest marker opens the timeline");
    }

    /// A lane renders from its own transcript whatever the join does. With no
    /// spawn to join to, the lane keeps every marker and the arrow falls back
    /// to the lane's own first row rather than disappearing.
    #[test]
    fn subagent_map_build_keeps_a_lane_that_joins_to_no_spawn() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), "");
        let mut cache = MapCache::default();
        let body = build_map(&mut cache, &dir, &parent, "root");

        assert_eq!(body.lanes.len(), 2, "the lane survives an empty parent file");
        let lane = &body.lanes[1];
        assert_eq!(lane.agent_id, "alane-fixture");
        assert_eq!(lane.started_at_ms, Some(LANE_ORIGIN_MS));
        assert!(
            body.events.iter().any(|e| e.lane == "alane-fixture" && e.kind == EventKind::Tool),
            "its own markers are untouched by the failed join"
        );
        let spawn = body
            .events
            .iter()
            .find(|e| e.kind == EventKind::Spawn)
            .expect("an arrow is still drawn");
        assert_eq!(spawn.at_ms, LANE_ORIGIN_MS, "timed by the lane itself");
    }

    /// A missing parent transcript is the ordinary case for a session whose
    /// agent writes under a config dir TUIC cannot read. It must cost the spawn
    /// timing, never the map.
    #[test]
    fn subagent_map_build_tolerates_a_missing_parent_transcript() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, _) = session_tree(tmp.path(), "");
        let mut cache = MapCache::default();
        let body = build_map(&mut cache, &dir, &tmp.path().join("gone.jsonl"), "root");
        assert_eq!(body.lanes.len(), 2);
    }

    /// The live half of the feature: a subagent spawned while the page is open
    /// must arrive as a new lane on the next poll, and its own lane must keep
    /// growing as it appends. The cursor is what makes the second half cheap and
    /// what would silently break it — a cached directory listing, or an offset
    /// that never revisits a file it has already read, both pass every other
    /// test here.
    #[test]
    fn subagent_map_build_picks_up_a_lane_that_appears_between_polls() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), PARENT_JSONL);
        let mut cache = MapCache::default();
        assert_eq!(build_map(&mut cache, &dir, &parent, "root").lanes.len(), 2);

        // A second subagent starts: its meta lands first, its transcript grows
        // afterwards — the order Claude writes them in.
        std::fs::write(dir.join("agent-alate.meta.json"), fixture("minimal")).expect("meta");
        let late = dir.join("agent-alate.jsonl");
        std::fs::write(&late, "").expect("empty transcript");

        let body = build_map(&mut cache, &dir, &parent, "root");
        assert_eq!(body.lanes.len(), 3, "the new lane arrives on the next poll");
        let lane = body
            .lanes
            .iter()
            .find(|l| l.agent_id == "alate")
            .expect("the late lane is listed");
        assert_eq!(lane.started_at_ms, None, "it has written nothing yet");
        assert!(lane.running);

        std::fs::write(
            &late,
            "{\"timestamp\":\"2026-09-21T10:00:20.000Z\",\"message\":{\"content\":\
             [{\"type\":\"tool_use\",\"name\":\"Grep\"}]}}\n",
        )
        .expect("append the first row");
        let grown = build_map(&mut cache, &dir, &parent, "root");
        let markers: Vec<&LaneEvent> = grown
            .events
            .iter()
            .filter(|e| e.lane == "alate")
            .collect();
        assert_eq!(markers.len(), 1, "its first marker shows up");
        assert_eq!(markers[0].label, "Grep");
        assert_eq!(
            grown.lanes.iter().find(|l| l.agent_id == "alate").unwrap().started_at_ms,
            Some(LANE_ORIGIN_MS + 20_000),
        );
    }

    /// The whole point of the cache: a second poll over an unchanged tree
    /// re-reads nothing and still answers with the same map.
    #[test]
    fn subagent_map_build_is_stable_across_polls() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), PARENT_JSONL);
        let mut cache = MapCache::default();
        let first = build_map(&mut cache, &dir, &parent, "root");
        let second = build_map(&mut cache, &dir, &parent, "root");
        assert_eq!(first.lanes, second.lanes);
        assert_eq!(first.events, second.events);
        assert_eq!(first.origin_ms, second.origin_ms);
    }
}
