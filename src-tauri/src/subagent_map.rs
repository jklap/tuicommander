//! Call map — who called whom, and why.
//!
//! One tree holds two kinds of node: TUIC terminals, nested by the peer spawn
//! link TUIC records for `agent action=spawn`, and the Claude in-process
//! subagents of each terminal, nested by `parentAgentId`. Time is not an axis:
//! a node carries its state, its duration and a compact count of its tool calls.
//!
//! Claude records an `Agent` spawn in the *parent* transcript
//! (`<config>/projects/<cwd-slug>/<uuid>.jsonl`) but writes the subagent's own
//! turns to a separate file under `<uuid>/subagents/`. Measured over 582 real
//! transcripts: 833 spawns, and **zero** `isSidechain:true` rows in any parent
//! file. That is why a subagent is invisible in the terminal that spawned it,
//! and why the subagent file — not the parent — is the source of truth for a
//! node.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

/// One subagent, described by its `agent-<id>.meta.json`.
///
/// Only `agentType`, `description` and `spawnDepth` appear in all 928 sampled
/// metas, so every other field is optional and its absence is ordinary. The
/// timing fields start empty and are filled from the transcripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lane {
    pub agent_id: String,
    /// Display title: `name`, else `description`, else `agentType`, else the id.
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

/// Subagents kept per terminal. Past this the tree stops being readable long
/// before it stops being renderable.
pub(crate) const MAX_LANES: usize = 64;
/// Distinct tool names counted per subagent. A later name counts toward
/// `other_tools`, so an agent calling hundreds of MCP tools cannot grow the
/// cache without bound.
pub(crate) const MAX_TOOL_KINDS: usize = 32;
/// Tool badges on one node; the rest fold into `other_tools`.
pub(crate) const MAX_TOOL_BADGES: usize = 6;
/// A tool name is a short identifier; anything longer is not a name.
pub(crate) const MAX_LABEL_CHARS: usize = 64;
/// Node titles wrap on the page, but a title is still a title, not a body.
pub(crate) const MAX_TITLE_CHARS: usize = 120;
/// The prompt excerpt a node shows before it is expanded.
pub(crate) const PROMPT_SUMMARY_CHARS: usize = 200;
/// Cap on the two texts the join compares — a spawn's prompt and a lane's first
/// row — and so on the prompt the expand endpoint can return. Both mirror a file
/// that can be tens of megabytes, so they are bounded per item.
///
/// Truncation can only weaken the join, never break a node: a shortened prompt
/// is still contained in the message that contained the whole one. No real
/// prompt comes close.
const MAX_JOIN_TEXT_CHARS: usize = 16_384;

/// What one transcript row contributes to a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub at_ms: i64,
    /// The first `tool_use` block's name, if the row calls a tool.
    pub tool: Option<String>,
    /// An assistant row that calls no tool: the subagent answered. When it is
    /// the last row, the subagent has finished.
    pub reply: bool,
}

/// Parse one JSONL row.
///
/// `None` for a row that carries nothing — a malformed line, a timestamp that
/// will not parse, or a message with no content. A bad line is skipped, never
/// fatal: these files are read while Claude appends to them, and one
/// unparseable row must not cost the other 457.
pub(crate) fn parse_row(line: &str) -> Option<Row> {
    let row: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let at_ms = row_timestamp(&row)?;
    let message = row.get("message")?;
    let tool = match message.get("content")? {
        serde_json::Value::Array(blocks) => blocks.iter().find_map(|b| {
            (b.get("type").and_then(|t| t.as_str()) == Some("tool_use")).then(|| {
                truncate_chars(
                    b.get("name").and_then(|n| n.as_str()).unwrap_or("tool"),
                    MAX_LABEL_CHARS,
                )
            })
        }),
        serde_json::Value::String(_) => None,
        _ => return None,
    };
    let assistant = message.get("role").and_then(|r| r.as_str()) == Some("assistant");
    Some(Row {
        at_ms,
        reply: assistant && tool.is_none(),
        tool,
    })
}

/// The decoded text of a row: a string `content`, or its `text` blocks joined.
///
/// Decoded, not raw: the teammate join looks for the spawn prompt inside this
/// text, and in the raw JSON line every newline in the prompt is the two
/// characters `\n`, so a multi-line prompt would never be found.
fn row_text(line: &str) -> Option<String> {
    let row: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let text = match row.get("message")?.get("content")? {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    (!text.is_empty()).then(|| truncate_chars(&text, MAX_JOIN_TEXT_CHARS))
}

fn row_timestamp(row: &serde_json::Value) -> Option<i64> {
    row.get("timestamp")
        .and_then(|t| t.as_str())
        .and_then(iso_to_ms)
}

/// Everything the tree needs from one subagent transcript, accumulated row by
/// row so a cached incremental read and a one-shot parse cannot disagree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LaneSummary {
    /// Rows parsed so far.
    pub rows: usize,
    pub tools: BTreeMap<String, u32>,
    /// Calls to tools past `MAX_TOOL_KINDS` distinct names.
    pub other_tools: u32,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    /// The last row read was a reply. The lane's own last row is the only
    /// evidence a subagent finished: the parent's tool_result acknowledges the
    /// spawn for 691 of 795 sampled calls and never carries the report.
    pub finished: bool,
    /// Decoded text of the first row that carries any: the spawn prompt as the
    /// subagent received it. Never changes once written.
    pub first_text: String,
    /// Decoded text of the newest reply: the subagent's report once it has
    /// finished. Tool results are never kept — only what the subagent wrote.
    pub last_reply: String,
}

impl LaneSummary {
    /// Fold a chunk of whole lines into the summary. Returns the rows parsed.
    fn absorb(&mut self, text: &str) -> usize {
        let before = self.rows;
        for line in text.lines() {
            if self.first_text.is_empty()
                && let Some(first) = row_text(line)
            {
                self.first_text = first;
            }
            let Some(row) = parse_row(line) else { continue };
            self.rows += 1;
            self.started_at_ms.get_or_insert(row.at_ms);
            self.ended_at_ms = Some(row.at_ms);
            self.finished = row.reply;
            if row.reply
                && let Some(reply) = row_text(line)
            {
                self.last_reply = reply;
            }
            if let Some(tool) = row.tool {
                if let Some(n) = self.tools.get_mut(&tool) {
                    *n += 1;
                } else if self.tools.len() < MAX_TOOL_KINDS {
                    self.tools.insert(tool, 1);
                } else {
                    self.other_tools += 1;
                }
            }
        }
        self.rows - before
    }

    pub(crate) fn tool_calls(&self) -> u32 {
        self.tools.values().sum::<u32>() + self.other_tools
    }
}

/// Per-file read cursor, so a 2s poll re-reads only what the agent appended.
///
/// Parent transcripts get their own map. The biggest one on this machine is
/// 31 MB, so re-reading it for every poll would cost 15 MB/s of disk and JSON
/// parsing for a handful of new rows.
#[derive(Default)]
pub(crate) struct MapCache {
    lanes: HashMap<PathBuf, LaneCursor>,
    parents: HashMap<PathBuf, SpawnCursor>,
}

#[derive(Default)]
struct LaneCursor {
    offset: u64,
    summary: LaneSummary,
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
fn read_appended(path: &Path, offset: &mut u64) -> std::io::Result<Appended> {
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
    /// Read whatever has been appended to a subagent transcript since the last
    /// call. Returns how many rows were newly parsed — 0 when the file has not
    /// moved.
    pub(crate) fn ingest(&mut self, path: &Path) -> std::io::Result<usize> {
        let cursor = self.lanes.entry(path.to_path_buf()).or_default();
        let appended = read_appended(path, &mut cursor.offset)?;
        if appended.restarted {
            cursor.summary = LaneSummary::default();
        }
        Ok(cursor.summary.absorb(&appended.text))
    }

    /// Everything read from a subagent transcript so far.
    pub(crate) fn summary(&self, path: &Path) -> Option<&LaneSummary> {
        self.lanes.get(path).map(|c| &c.summary)
    }

    /// Read the `Agent` calls appended to a parent transcript since the last call.
    pub(crate) fn ingest_spawns(&mut self, path: &Path) -> std::io::Result<usize> {
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
    pub(crate) fn spawns(&self, path: &Path) -> &[ParentSpawn] {
        self.parents
            .get(path)
            .map(|c| c.spawns.as_slice())
            .unwrap_or(&[])
    }
}

fn iso_to_ms(ts: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| d.timestamp_millis())
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
    let at_ms = row_timestamp(&row)?;
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
    /// Decoded text of the lane's first row.
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
/// The 2 that match neither are left unmapped on purpose. A node renders from
/// its own transcript, so an absent mapping costs the spawn time and the joined
/// prompt and nothing else; inventing a link would draw a confidently wrong one.
pub(crate) fn join_spawns(
    spawns: &[ParentSpawn],
    lanes: &[LaneProbe<'_>],
) -> HashMap<String, String> {
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
    let meta: serde_json::Map<String, serde_json::Value> = serde_json::from_str(meta_json).ok()?;
    let string = |key: &str| {
        meta.get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let agent_type = string("agentType").unwrap_or_default();
    let description = string("description").unwrap_or_default();
    Some(Lane {
        // `name` is absent from 538 of 928 metas. The description says what the
        // subagent is for; the agent type only says what kind it is, and a
        // fan-out of `general-purpose` titles tells the reader nothing.
        name: string("name")
            .or_else(|| (!description.is_empty()).then(|| description.clone()))
            .or_else(|| (!agent_type.is_empty()).then(|| agent_type.clone()))
            .unwrap_or_else(|| agent_id.to_owned()),
        agent_id: agent_id.to_owned(),
        description,
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

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NodeKind {
    Terminal,
    Subagent,
}

/// `running`/`done` for a subagent; `busy`/`idle`/`awaiting` for a terminal.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NodeState {
    Running,
    Done,
    Busy,
    Idle,
    Awaiting,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ToolCount {
    pub name: String,
    pub count: u32,
}

/// One node of the call map.
///
/// Every field is final text or a number: the page renders, it never derives.
/// `agent_type` and `description` are `None` when they would only repeat
/// `title`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct MapNode {
    pub kind: NodeKind,
    /// PTY session id for a terminal, agent id for a subagent.
    pub id: String,
    /// The terminal the node belongs to — the key of the prompt endpoint.
    pub session_id: String,
    pub title: String,
    pub agent_type: Option<String>,
    pub model: Option<String>,
    pub description: Option<String>,
    /// The terminal's current TUIC `intent:` text.
    pub intent: Option<String>,
    pub state: NodeState,
    pub started_at_ms: Option<i64>,
    /// End minus start, or now minus start while running.
    pub duration_ms: Option<i64>,
    /// Redacted, whitespace-collapsed, at most `PROMPT_SUMMARY_CHARS`.
    pub prompt_summary: Option<String>,
    /// The summary is shorter than the prompt, so expanding shows more.
    pub prompt_more: bool,
    /// Most-called first, at most `MAX_TOOL_BADGES`.
    pub tools: Vec<ToolCount>,
    /// Calls not shown in `tools`.
    pub other_tools: u32,
    pub tool_calls: u32,
    pub children: Vec<MapNode>,
}

/// Redact first, then shorten. Cutting first could split a secret so its
/// pattern no longer matches, and its prefix would reach the page.
///
/// Returns the summary and whether it dropped anything.
pub(crate) fn prompt_summary(prompt: &str) -> (Option<String>, bool) {
    let redacted = crate::redaction::redact_secrets(prompt);
    let flat = redacted.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return (None, false);
    }
    let more = flat.chars().count() > PROMPT_SUMMARY_CHARS;
    (Some(truncate_chars(&flat, PROMPT_SUMMARY_CHARS)), more)
}

/// Most-called first, ties by name so the order is stable between polls.
fn tool_badges(summary: &LaneSummary) -> (Vec<ToolCount>, u32) {
    let mut all: Vec<ToolCount> = summary
        .tools
        .iter()
        .map(|(name, &count)| ToolCount {
            name: name.clone(),
            count,
        })
        .collect();
    all.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    let hidden: u32 = all.iter().skip(MAX_TOOL_BADGES).map(|t| t.count).sum();
    all.truncate(MAX_TOOL_BADGES);
    (all, hidden + summary.other_tools)
}

fn duration(start: Option<i64>, end: Option<i64>) -> Option<i64> {
    Some((end? - start?).max(0))
}

/// A lane with everything read about it, before it becomes a node.
struct LaneData {
    lane: Lane,
    summary: LaneSummary,
    /// The joined spawn prompt, else the lane's own first message. Unredacted:
    /// it never leaves the process in this form.
    prompt: String,
}

/// Read every subagent of one terminal and join it to its spawn.
///
/// `parent_transcript` supplies the spawn time and the prompt. Its absence is
/// not an error: a node renders from its own transcript whatever the join does.
fn collect_lanes(
    cache: &mut MapCache,
    subagents_dir: &Path,
    parent_transcript: &Path,
) -> Vec<LaneData> {
    let mut found = lane_files(subagents_dir);
    // By id first, so two lanes that started in the same millisecond — or that
    // have not started at all — keep a stable order between polls.
    found.sort_by(|a, b| a.0.cmp(&b.0));

    let mut lanes: Vec<LaneData> = Vec::new();
    for (agent_id, meta_path, jsonl_path) in found {
        let Ok(meta) = std::fs::read_to_string(&meta_path) else {
            continue;
        };
        let Some(lane) = parse_lane_meta(&agent_id, &meta) else {
            continue;
        };
        // A meta appears a moment before the transcript it describes, so a
        // missing or unreadable transcript is an empty node, not a lost one.
        let _ = cache.ingest(&jsonl_path);
        lanes.push(LaneData {
            lane,
            summary: cache.summary(&jsonl_path).cloned().unwrap_or_default(),
            prompt: String::new(),
        });
    }

    // Oldest first (a stable sort keeps the id order for ties), and capped.
    lanes.sort_by_key(|d| d.summary.started_at_ms.unwrap_or(i64::MAX));
    lanes.truncate(MAX_LANES);

    let _ = cache.ingest_spawns(parent_transcript);
    let spawns = cache.spawns(parent_transcript);
    let joined = {
        let probes: Vec<LaneProbe<'_>> = lanes
            .iter()
            .map(|d| LaneProbe {
                agent_id: &d.lane.agent_id,
                tool_use_id: d.lane.tool_use_id.as_deref(),
                first_message: &d.summary.first_text,
            })
            .collect();
        join_spawns(spawns, &probes)
    };

    for d in &mut lanes {
        let spawn = joined
            .get(&d.lane.agent_id)
            .and_then(|id| spawns.iter().find(|s| &s.tool_use_id == id));
        // The parent's own tool call is the better start — the child's first
        // row lands after it.
        d.lane.started_at_ms = spawn.map(|s| s.at_ms).or(d.summary.started_at_ms);
        if d.summary.finished
            && let Some(end) = d.summary.ended_at_ms
        {
            d.lane.finish(end);
        }
        d.prompt = spawn
            .map(|s| s.prompt.clone())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| d.summary.first_text.clone());
    }
    lanes
}

fn subagent_node(data: LaneData, session_id: &str, now_ms: i64) -> MapNode {
    let LaneData {
        lane,
        summary,
        prompt,
    } = data;
    let (prompt_summary, prompt_more) = prompt_summary(&prompt);
    let (tools, other_tools) = tool_badges(&summary);
    let end = lane.ended_at_ms.or(lane.running.then_some(now_ms));
    let agent_type = Some(lane.agent_type).filter(|t| !t.is_empty() && *t != lane.name);
    let description = Some(lane.description).filter(|d| !d.is_empty() && *d != lane.name);
    MapNode {
        kind: NodeKind::Subagent,
        id: lane.agent_id,
        session_id: session_id.to_owned(),
        title: truncate_chars(&lane.name, MAX_TITLE_CHARS),
        agent_type,
        model: lane.model,
        description,
        intent: None,
        state: if lane.running {
            NodeState::Running
        } else {
            NodeState::Done
        },
        started_at_ms: lane.started_at_ms,
        duration_ms: duration(lane.started_at_ms, end),
        prompt_summary,
        prompt_more,
        tools,
        other_tools,
        tool_calls: summary.tool_calls(),
        children: Vec::new(),
    }
}

/// The subagents of one terminal, nested by `parentAgentId`.
///
/// A subagent whose parent is not among the lanes read hangs off the terminal
/// itself: it renders from its own transcript whatever its parent does.
pub(crate) fn subagent_nodes(
    cache: &mut MapCache,
    subagents_dir: &Path,
    parent_transcript: &Path,
    session_id: &str,
    now_ms: i64,
) -> Vec<MapNode> {
    let entries = collect_lanes(cache, subagents_dir, parent_transcript)
        .into_iter()
        .map(|d| {
            let id = d.lane.agent_id.clone();
            let parent = d.lane.parent_agent_id.clone();
            (id, parent, subagent_node(d, session_id, now_ms))
        })
        .collect();
    nest(entries)
}

/// Which of a subagent's two texts to return in full.
#[derive(Clone, Copy, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TextPart {
    /// What it was asked to do.
    Prompt,
    /// What it answered last.
    Report,
}

/// One subagent's full prompt or report, redacted, for an expand request.
///
/// `agent_id` is compared against the lanes found on disk; it never becomes
/// part of a path. `None` for an unknown id or an empty text.
pub(crate) fn subagent_text(
    cache: &mut MapCache,
    subagents_dir: &Path,
    parent_transcript: &Path,
    agent_id: &str,
    part: TextPart,
) -> Option<String> {
    collect_lanes(cache, subagents_dir, parent_transcript)
        .into_iter()
        .find(|d| d.lane.agent_id == agent_id)
        .map(|d| match part {
            TextPart::Prompt => d.prompt,
            TextPart::Report => d.summary.last_reply,
        })
        .map(|text| crate::redaction::redact_secrets(&text))
        .filter(|text| !text.trim().is_empty())
}

/// One subagent as the Progress Flow view needs it: who it is, when it was
/// handed its task and when it returned. The texts are unredacted here; the
/// flow builder redacts and shortens them before anything reaches the wire.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SubagentFlow {
    pub agent_id: String,
    pub parent_agent_id: Option<String>,
    pub title: String,
    pub agent_type: Option<String>,
    pub running: bool,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    pub tool_calls: u32,
    pub prompt: String,
    pub report: String,
}

/// Every subagent of one terminal, for the Progress Flow view.
pub(crate) fn subagent_flows(
    cache: &mut MapCache,
    subagents_dir: &Path,
    parent_transcript: &Path,
) -> Vec<SubagentFlow> {
    collect_lanes(cache, subagents_dir, parent_transcript)
        .into_iter()
        .map(|d| SubagentFlow {
            agent_type: Some(d.lane.agent_type.clone())
                .filter(|t| !t.is_empty() && *t != d.lane.name),
            title: truncate_chars(&d.lane.name, MAX_TITLE_CHARS),
            agent_id: d.lane.agent_id,
            parent_agent_id: d.lane.parent_agent_id,
            running: d.lane.running,
            started_at_ms: d.lane.started_at_ms,
            ended_at_ms: d.lane.ended_at_ms,
            tool_calls: d.summary.tool_calls(),
            prompt: d.prompt,
            report: d.summary.last_reply,
        })
        .collect()
}

/// Where one TUIC session's Claude transcripts live on disk.
pub(crate) struct TranscriptSource {
    pub subagents_dir: PathBuf,
    pub parent_transcript: PathBuf,
}

/// Resolve a TUIC session id to the files its subagents write.
///
/// The id is a key into `AppState` and never reaches the filesystem: every path
/// component comes from the session's own cwd, the agent process's
/// `CLAUDE_CONFIG_DIR`, and the session uuid Claude itself published. An unknown
/// id therefore resolves to `None` rather than to a path.
///
/// `None` is the ordinary answer — for a shell tab, for a Claude tab that has
/// spawned nothing, and for a session whose agent has exited.
pub(crate) fn transcript_source(
    state: &crate::state::AppState,
    session_id: &str,
) -> Option<TranscriptSource> {
    // Gate on the agent type TUIC already detected. Claude discovery falls back
    // to "newest unclaimed session file under the project dir" when the pid is
    // not in Claude's registry, so asking it about a shell tab would hand this
    // session another tab's transcript (issue #119).
    let is_claude = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| s.agent_type.clone())
        .is_some_and(|t| t == "claude");
    if !is_claude {
        return None;
    }

    let cwd = {
        let entry = state.session_maps.sessions.get(session_id)?;
        let session = entry.value().lock();
        session.cwd.clone()?
    };
    let pid = crate::pty::session_leaf_pid(state, session_id)?;
    let config_dir =
        crate::agent_session::read_agent_env_overrides("claude", pid).remove("CLAUDE_CONFIG_DIR");
    let uuid = crate::agent_session::discover_agent_session(
        "claude".to_owned(),
        cwd.clone(),
        Vec::new(),
        Some(pid),
        HashMap::new(),
    )?
    .session_id;

    Some(TranscriptSource {
        subagents_dir: subagents_dir(&cwd, config_dir.as_deref(), &uuid)?,
        parent_transcript: crate::agent_session::claude_project_dir_path(
            &cwd,
            config_dir.as_deref(),
        )?
        .join(format!("{uuid}.jsonl")),
    })
}

/// One terminal as the handler found it in `AppState`.
pub(crate) struct TerminalInfo {
    pub session_id: String,
    pub title: String,
    pub agent_type: Option<String>,
    pub intent: Option<String>,
    pub state: NodeState,
    /// PTY session id of the terminal that spawned this one with
    /// `agent action=spawn`, if TUIC recorded it and that terminal still lives.
    pub parent_session: Option<String>,
    pub subagents: Vec<MapNode>,
}

/// The whole call map.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub(crate) struct CallMap {
    pub roots: Vec<MapNode>,
    pub terminals: usize,
    pub subagents: usize,
    /// Subagents still running.
    pub running: usize,
}

/// Nest terminals by their spawn link. A terminal's own subagents come first
/// among its children, then the peers it spawned.
pub(crate) fn assemble(mut terminals: Vec<TerminalInfo>) -> CallMap {
    terminals.sort_by(|a, b| {
        a.title
            .cmp(&b.title)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    let entries = terminals
        .into_iter()
        .map(|t| {
            let title = truncate_chars(&t.title, MAX_TITLE_CHARS);
            let node = MapNode {
                kind: NodeKind::Terminal,
                id: t.session_id.clone(),
                session_id: t.session_id.clone(),
                agent_type: t.agent_type.filter(|a| *a != title),
                title,
                model: None,
                description: None,
                intent: t.intent.filter(|i| !i.trim().is_empty()),
                state: t.state,
                started_at_ms: None,
                duration_ms: None,
                prompt_summary: None,
                prompt_more: false,
                tools: Vec::new(),
                other_tools: 0,
                tool_calls: 0,
                children: t.subagents,
            };
            (t.session_id, t.parent_session, node)
        })
        .collect();

    let mut map = CallMap {
        roots: nest(entries),
        ..CallMap::default()
    };
    let mut stack: Vec<&MapNode> = map.roots.iter().collect();
    let (mut terminals, mut subagents, mut running) = (0, 0, 0);
    while let Some(node) = stack.pop() {
        match node.kind {
            NodeKind::Terminal => terminals += 1,
            NodeKind::Subagent => subagents += 1,
        }
        if node.state == NodeState::Running {
            running += 1;
        }
        stack.extend(node.children.iter());
    }
    map.terminals = terminals;
    map.subagents = subagents;
    map.running = running;
    map
}

/// Hang each node under its parent, keeping input order among siblings and
/// after any children the node already carries.
///
/// A node whose parent is unknown is a root. So is every node on a parent
/// cycle: the links come from files and process state, not from a structure
/// that guarantees a tree, and a cycle must cost the edge, never the node.
fn nest(entries: Vec<(String, Option<String>, MapNode)>) -> Vec<MapNode> {
    let n = entries.len();
    let index: HashMap<&str, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, (id, _, _))| (id.as_str(), i))
        .collect();
    let parent_of: Vec<Option<usize>> = entries
        .iter()
        .enumerate()
        .map(|(i, (_, parent, _))| {
            parent
                .as_deref()
                .and_then(|p| index.get(p).copied())
                .filter(|&p| p != i)
        })
        .collect();
    let on_cycle = |i: usize| {
        let mut cur = parent_of[i];
        for _ in 0..n {
            match cur {
                Some(p) if p == i => return true,
                Some(p) => cur = parent_of[p],
                None => return false,
            }
        }
        false
    };
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut roots: Vec<usize> = Vec::new();
    for i in 0..n {
        match parent_of[i].filter(|_| !on_cycle(i)) {
            Some(p) => kids[p].push(i),
            None => roots.push(i),
        }
    }

    fn build(i: usize, slots: &mut [Option<MapNode>], kids: &[Vec<usize>]) -> MapNode {
        let mut node = slots[i].take().expect("each node is placed exactly once");
        for &k in &kids[i] {
            let child = build(k, slots, kids);
            node.children.push(child);
        }
        node
    }
    let mut slots: Vec<Option<MapNode>> =
        entries.into_iter().map(|(_, _, node)| Some(node)).collect();
    roots
        .into_iter()
        .map(|r| build(r, &mut slots, &kids))
        .collect()
}

/// Every `agent-<id>.meta.json` in a subagents dir, with the transcript beside
/// it. The row's own `agentId` is the file name minus the `agent-` prefix, which
/// is what `parentAgentId` on another lane points at.
fn lane_files(dir: &Path) -> Vec<(String, PathBuf, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".meta.json")?;
            let agent_id = stem.strip_prefix("agent-")?.to_owned();
            Some((agent_id, entry.path(), dir.join(format!("{stem}.jsonl"))))
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
    /// (`parentAgentId`). Both must survive parsing — the first joins the
    /// spawn prompt, the second decides which node the subagent hangs under.
    #[test]
    fn subagent_map_lane_reads_a_fork_meta() {
        let lane = parse_lane_meta("acap-advisor-abc", fixture("fork")).expect("parses");
        assert_eq!(lane.agent_id, "acap-advisor-abc");
        assert_eq!(lane.name, "cap-advisor");
        assert_eq!(lane.agent_type, "fork");
        assert_eq!(lane.description, "Second opinion on the cap");
        assert_eq!(lane.model.as_deref(), Some("inherit"));
        assert_eq!(lane.tool_use_id.as_deref(), Some("toolu_FIXTURE_FORK"));
        assert_eq!(
            lane.parent_agent_id.as_deref(),
            Some("aroot-0000000000000001")
        );
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

    /// `name` is absent from 538 of 928 metas. The old fallback went straight to
    /// `agentType`, which is how a fan-out drew nine cards all reading
    /// "general-purpose / general-purpose". The description says what the
    /// subagent is for, so it comes first.
    #[test]
    fn subagent_map_lane_name_falls_back_to_description_then_type_then_id() {
        let named = parse_lane_meta("aid-1", fixture("fork")).expect("parses");
        assert_eq!(named.name, "cap-advisor", "an explicit name wins");

        let unnamed = parse_lane_meta("aid-2", fixture("minimal")).expect("parses");
        assert_eq!(
            unnamed.name, "Scan the tree",
            "falls back to the description"
        );

        let typed =
            parse_lane_meta("aid-3", r#"{"agentType":"Explore","spawnDepth":0}"#).expect("parses");
        assert_eq!(typed.name, "Explore", "then to agentType");

        let bare = parse_lane_meta("aid-4", r#"{"spawnDepth":0}"#).expect("parses");
        assert_eq!(bare.name, "aid-4", "then to the agent id");
    }

    #[test]
    fn subagent_map_lane_rejects_meta_that_is_not_an_object() {
        assert!(parse_lane_meta("aid", "not json").is_none());
        assert!(parse_lane_meta("aid", "[]").is_none());
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

    fn row(ts: &str, tool: &str) -> String {
        format!(
            r#"{{"timestamp":"{ts}","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"x","name":"{tool}"}}]}}}}"#
        )
    }

    fn append(path: &Path, text: &str) {
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
    /// incremental read from a full one that happens to return the same totals.
    #[test]
    fn subagent_map_cursor_parses_only_appended_rows() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("agent-a.jsonl");
        append(
            &path,
            &format!(
                "{}\n{}\n",
                row("2026-09-21T10:00:00Z", "Read"),
                row("2026-09-21T10:00:01Z", "Bash")
            ),
        );

        let mut cache = MapCache::default();
        assert_eq!(
            cache.ingest(&path).expect("read"),
            2,
            "first read parses both rows"
        );
        assert_eq!(
            cache.ingest(&path).expect("read"),
            0,
            "nothing changed, nothing parsed"
        );

        append(&path, &format!("{}\n", row("2026-09-21T10:00:02Z", "Read")));
        assert_eq!(
            cache.ingest(&path).expect("read"),
            1,
            "only the appended row is parsed"
        );
        let summary = cache.summary(&path).expect("cached");
        assert_eq!(
            summary.rows, 3,
            "the accumulated summary still counts all three"
        );
        assert_eq!(summary.tools.get("Read"), Some(&2));
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
        assert_eq!(
            cache.ingest(&path).expect("read"),
            1,
            "only the finished line"
        );

        append(&path, &format!("{tail}\n"));
        assert_eq!(
            cache.ingest(&path).expect("read"),
            1,
            "the completed line parses now"
        );
        let summary = cache.summary(&path).expect("cached");
        assert_eq!(
            summary.tools.get("Bash"),
            Some(&1),
            "and exactly once — not twice"
        );
    }

    /// A stale offset into a shorter file reads from the middle of a line.
    #[test]
    fn subagent_map_cursor_resets_when_the_file_shrinks() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("agent-a.jsonl");
        append(
            &path,
            &format!(
                "{}\n{}\n{}\n",
                row("2026-09-21T10:00:00Z", "Read"),
                row("2026-09-21T10:00:01Z", "Bash"),
                row("2026-09-21T10:00:02Z", "Write")
            ),
        );

        let mut cache = MapCache::default();
        assert_eq!(cache.ingest(&path).expect("read"), 3);

        std::fs::write(&path, format!("{}\n", row("2026-09-21T11:00:00Z", "Glob")))
            .expect("truncate");
        assert_eq!(cache.ingest(&path).expect("read"), 1, "re-reads from zero");
        let summary = cache.summary(&path).expect("cached");
        assert_eq!(summary.rows, 1, "the stale rows are dropped, not added to");
        assert_eq!(summary.tools.keys().collect::<Vec<_>>(), vec!["Glob"]);
    }

    #[test]
    fn subagent_map_cursor_keeps_lanes_apart_by_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let a = tmp.path().join("agent-a.jsonl");
        let b = tmp.path().join("agent-b.jsonl");
        append(&a, &format!("{}\n", row("2026-09-21T10:00:00Z", "Read")));
        append(
            &b,
            &format!(
                "{}\n{}\n",
                row("2026-09-21T10:00:00Z", "Bash"),
                row("2026-09-21T10:00:01Z", "Bash")
            ),
        );

        let mut cache = MapCache::default();
        cache.ingest(&a).expect("read");
        cache.ingest(&b).expect("read");
        assert_eq!(cache.summary(&a).expect("a").tool_calls(), 1);
        assert_eq!(cache.summary(&b).expect("b").tools.get("Bash"), Some(&2));
    }

    const LANE_JSONL: &str = include_str!("fixtures/subagent_map/lane.jsonl");

    /// Summary of one whole transcript, through the same `absorb` the cursor
    /// uses, so a one-shot parse and an incremental read cannot disagree.
    fn summarize(jsonl: &str) -> LaneSummary {
        let mut summary = LaneSummary::default();
        summary.absorb(jsonl);
        summary
    }
    /// 2026-09-21T10:00:00.000Z — the fixture's first row.
    const LANE_ORIGIN_MS: i64 = 1_789_984_800_000;

    /// Tool activity is a count per tool, not a row per call: that is what
    /// keeps a node one card tall however long the subagent ran.
    #[test]
    fn subagent_map_summary_counts_calls_per_tool() {
        let s = summarize(LANE_JSONL);
        assert_eq!(s.tools.get("Read"), Some(&2));
        assert_eq!(s.tools.get("Bash"), Some(&3));
        assert_eq!(s.tool_calls(), 5);
    }

    /// The parent's `tool_result` is `{status: teammate_spawned}` for 358 of the
    /// 795 sampled spawns and never carries the report, so the end of a
    /// subagent can only come from its own last row.
    #[test]
    fn subagent_map_summary_takes_the_end_from_the_lanes_own_last_row() {
        let s = summarize(LANE_JSONL);
        assert_eq!(s.started_at_ms, Some(LANE_ORIGIN_MS));
        assert_eq!(s.ended_at_ms, Some(LANE_ORIGIN_MS + 9_250));
        assert!(s.finished, "the last row is an assistant reply");
    }

    /// A subagent waiting on a tool is working, not done. The old heuristic
    /// counted any non-tool row as a finish, so a tool_result — a *user* row —
    /// marked a subagent done while it waited for its next turn.
    #[test]
    fn subagent_map_summary_is_not_finished_while_a_tool_result_is_last() {
        let calling = format!("{}\n", row("2026-09-21T10:00:00Z", "Bash"));
        assert!(!summarize(&calling).finished, "a tool call is in flight");

        let result = r#"{"timestamp":"2026-09-21T10:00:01Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"x","content":"ok"}]}}"#;
        assert!(
            !summarize(&format!("{calling}{result}\n")).finished,
            "the tool answered; the subagent has not"
        );
    }

    /// Distinct tool names are bounded: an agent calling many MCP tools must
    /// not grow one cached summary without limit, and no call is lost.
    #[test]
    fn subagent_map_summary_caps_distinct_tool_names_without_losing_calls() {
        let mut rows = String::new();
        for i in 0..(MAX_TOOL_KINDS + 5) {
            rows.push_str(&row("2026-09-21T10:00:00Z", &format!("mcp__t{i:03}")));
            rows.push('\n');
        }
        let s = summarize(&rows);
        assert_eq!(s.tools.len(), MAX_TOOL_KINDS);
        assert_eq!(s.other_tools, 5);
        assert_eq!(s.tool_calls() as usize, MAX_TOOL_KINDS + 5);
    }

    /// The report is the last thing the subagent wrote itself. A tool result
    /// after it is input the subagent received, not something it said.
    #[test]
    fn subagent_map_summary_keeps_the_last_reply_and_never_a_tool_result() {
        let s = summarize(LANE_JSONL);
        assert_eq!(s.last_reply, "Done. Two findings, both minor.");

        let result = r#"{"timestamp":"2026-09-21T10:00:10Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"x","content":"RESULT-BODY"}]}}"#;
        let s = summarize(&format!("{LANE_JSONL}{result}\n"));
        assert_eq!(s.last_reply, "Done. Two findings, both minor.");
    }

    #[test]
    fn subagent_map_summary_skips_a_malformed_row_without_losing_the_rest() {
        let rows = format!("not json\n{{\"broken\":\n{LANE_JSONL}");
        let s = summarize(&rows);
        assert_eq!(
            s.tools.get("Bash"),
            Some(&3),
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
            LaneProbe {
                agent_id: "lane-db",
                tool_use_id: None,
                first_message: "…Review src/db.rs…",
            },
            LaneProbe {
                agent_id: "lane-auth",
                tool_use_id: None,
                first_message: "…Review src/auth.rs…",
            },
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
            LaneProbe {
                agent_id: "lane-1",
                tool_use_id: None,
                first_message: "same prompt",
            },
            LaneProbe {
                agent_id: "lane-2",
                tool_use_id: None,
                first_message: "same prompt",
            },
        ];
        let joined = join_spawns(&spawns, &lanes);
        assert_eq!(joined.len(), 1, "one spawn, one lane: {joined:?}");
    }

    /// The failure that must stay harmless. 2 of 928 metas join to nothing; the
    /// caller still has to render their nodes, so the join reports absence
    /// rather than inventing a link.
    #[test]
    fn subagent_map_join_leaves_an_unmatched_lane_unmapped() {
        let spawns = [spawn("toolu_A", 10, "alpha")];
        let lanes = [
            LaneProbe {
                agent_id: "orphan",
                tool_use_id: None,
                first_message: "unrelated",
            },
            LaneProbe {
                agent_id: "ghost",
                tool_use_id: Some("toolu_GONE"),
                first_message: "",
            },
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
        let lanes = [LaneProbe {
            agent_id: "lane-1",
            tool_use_id: None,
            first_message: "anything",
        }];
        assert!(join_spawns(&spawns, &lanes).is_empty());
    }

    /// The slug, the uuid and `subagents` must arrive as three separate path
    /// components. Asserting the rendered string instead would pass on unix and
    /// fail on Windows for a path that is in fact correct — the separator is the
    /// host's business, the components are ours.
    fn tail_components(path: &Path, n: usize) -> Vec<String> {
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
        let path =
            subagents_path("/Users/stefano.straus/Gits/p", Some("/c"), "u").expect("resolves");
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
        assert_eq!(
            subagents_dir("/Users/foo/bar", Some(&cfg), "uuid-1"),
            Some(dir)
        );
    }

    const PARENT_JSONL: &str = include_str!("fixtures/subagent_map/parent.jsonl");
    const NOW_MS: i64 = LANE_ORIGIN_MS + 60_000;

    /// A subagents dir holding one lane, beside the parent transcript that
    /// spawned it. Returns (subagents dir, parent transcript).
    fn session_tree(tmp: &Path, parent: &str) -> (PathBuf, PathBuf) {
        let dir = tmp.join("subagents");
        std::fs::create_dir_all(&dir).expect("create the subagents dir");
        std::fs::write(
            dir.join("agent-alane-fixture.meta.json"),
            fixture("teammate"),
        )
        .expect("write the meta");
        std::fs::write(dir.join("agent-alane-fixture.jsonl"), LANE_JSONL)
            .expect("write the lane transcript");
        let transcript = tmp.join("parent.jsonl");
        std::fs::write(&transcript, parent).expect("write the parent transcript");
        (dir, transcript)
    }

    fn nodes(dir: &Path, parent: &Path) -> Vec<MapNode> {
        subagent_nodes(&mut MapCache::default(), dir, parent, "sess-1", NOW_MS)
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

    /// The node answers "who, and why" on its own: a title that is not the
    /// agent type repeated, the description, the prompt, a done badge with a
    /// duration, and tool counts instead of one row per call.
    #[test]
    fn subagent_map_node_says_what_the_subagent_was_for() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), PARENT_JSONL);
        let tree = nodes(&dir, &parent);

        assert_eq!(tree.len(), 1);
        let n = &tree[0];
        assert_eq!(n.kind, NodeKind::Subagent);
        assert_eq!(n.id, "alane-fixture");
        assert_eq!(n.session_id, "sess-1", "the key the prompt endpoint needs");
        assert_eq!(n.title, "reviewer-x");
        assert_eq!(n.agent_type.as_deref(), Some("reviewer"));
        assert_eq!(n.description.as_deref(), Some("Review the diff"));
        assert_eq!(
            n.model.as_deref(),
            Some("haiku"),
            "the model is not truncated"
        );
        assert_eq!(
            n.prompt_summary.as_deref(),
            Some("Review the diff and report")
        );
        assert!(!n.prompt_more, "the whole prompt already fits");
        assert_eq!(n.state, NodeState::Done);
        assert_eq!(
            n.started_at_ms,
            Some(LANE_ORIGIN_MS - 2_000),
            "the parent's Agent call starts the node"
        );
        assert_eq!(
            n.duration_ms,
            Some(11_250),
            "from the spawn to the last reply"
        );
        assert_eq!(
            n.tools,
            vec![
                ToolCount {
                    name: "Bash".into(),
                    count: 3
                },
                ToolCount {
                    name: "Read".into(),
                    count: 2
                },
            ],
            "most-called first"
        );
        assert_eq!(n.tool_calls, 5);
    }

    /// "general-purpose / general-purpose" was the defect: the card printed
    /// the fallback title and then the type it fell back to.
    #[test]
    fn subagent_map_node_never_repeats_the_title() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), "");
        std::fs::write(dir.join("agent-ascan.meta.json"), fixture("minimal")).expect("meta");
        std::fs::write(
            dir.join("agent-atyped.meta.json"),
            r#"{"agentType":"general-purpose","spawnDepth":0}"#,
        )
        .expect("meta");
        let tree = nodes(&dir, &parent);

        let scan = tree.iter().find(|n| n.id == "ascan").expect("scan node");
        assert_eq!(scan.title, "Scan the tree");
        assert_eq!(
            scan.description, None,
            "the description is already the title"
        );
        assert_eq!(scan.agent_type.as_deref(), Some("general-purpose"));

        let typed = tree.iter().find(|n| n.id == "atyped").expect("typed node");
        assert_eq!(typed.title, "general-purpose");
        assert_eq!(typed.agent_type, None, "the type is already the title");
    }

    /// A running node's duration grows with the clock, so the badge is live
    /// without the page doing arithmetic.
    #[test]
    fn subagent_map_node_times_a_running_subagent_against_now() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), "");
        std::fs::write(
            dir.join("agent-alane-fixture.jsonl"),
            format!("{}\n", row("2026-09-21T10:00:00Z", "Bash")),
        )
        .expect("lane");
        let n = &nodes(&dir, &parent)[0];
        assert_eq!(n.state, NodeState::Running);
        assert_eq!(n.duration_ms, Some(NOW_MS - LANE_ORIGIN_MS));
    }

    /// Nesting follows `parentAgentId`. A parent that is not on disk costs the
    /// edge, not the node: the child hangs off the terminal instead.
    #[test]
    fn subagent_map_nodes_nest_by_parent_agent_id() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), "");
        std::fs::write(
            dir.join("agent-aroot-0000000000000001.meta.json"),
            fixture("minimal"),
        )
        .expect("meta");
        std::fs::write(dir.join("agent-afork.meta.json"), fixture("fork")).expect("meta");
        std::fs::write(
            dir.join("agent-aorphan.meta.json"),
            r#"{"agentType":"x","description":"orphan","parentAgentId":"agone","spawnDepth":1}"#,
        )
        .expect("meta");
        let tree = nodes(&dir, &parent);

        let root_ids: Vec<&str> = tree.iter().map(|n| n.id.as_str()).collect();
        assert!(root_ids.contains(&"aroot-0000000000000001"));
        assert!(
            root_ids.contains(&"aorphan"),
            "an unknown parent makes a root"
        );
        assert!(
            !root_ids.contains(&"afork"),
            "the fork is nested, not a root"
        );
        let root = tree
            .iter()
            .find(|n| n.id == "aroot-0000000000000001")
            .expect("root");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].id, "afork");
    }

    /// Real prompts span lines. The teammate join must compare decoded text,
    /// not the raw JSON line where a newline is the two characters `\n` —
    /// with the raw line, every multi-line teammate prompt failed to join.
    #[test]
    fn subagent_map_build_joins_a_teammate_whose_prompt_spans_lines() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let prompt = "Review the diff\nand report \"findings\"";
        let parent = serde_json::json!({"timestamp":"2026-09-21T09:59:58.000Z","message":{"content":[{"type":"tool_use","id":"toolu_ML","name":"Agent","input":{"prompt":prompt}}]}}).to_string();
        let first = serde_json::json!({"timestamp":"2026-09-21T10:00:00.000Z","message":{"role":"user","content":format!("<teammate-message>\n{prompt}\n</teammate-message>")}}).to_string();
        let (dir, transcript) = session_tree(tmp.path(), &format!("{parent}\n"));
        std::fs::write(dir.join("agent-alane-fixture.jsonl"), format!("{first}\n")).expect("lane");
        let n = &nodes(&dir, &transcript)[0];
        assert_eq!(
            n.started_at_ms,
            Some(LANE_ORIGIN_MS - 2_000),
            "timed by the joined parent call"
        );
        assert_eq!(
            n.prompt_summary.as_deref(),
            Some("Review the diff and report \"findings\""),
            "the joined prompt, not the teammate wrapper"
        );
    }

    /// A node renders from its own transcript whatever the join does. With no
    /// spawn to join to, the node keeps its counts and times itself.
    #[test]
    fn subagent_map_build_keeps_a_lane_that_joins_to_no_spawn() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), "");
        let n = &nodes(&dir, &parent)[0];
        assert_eq!(n.id, "alane-fixture");
        assert_eq!(
            n.started_at_ms,
            Some(LANE_ORIGIN_MS),
            "timed by the lane itself"
        );
        assert_eq!(
            n.tool_calls, 5,
            "its own counts are untouched by the failed join"
        );
        assert!(
            n.prompt_summary
                .as_deref()
                .is_some_and(|p| p.contains("Review the diff and report")),
            "the lane's own first message stands in for the prompt"
        );
    }

    /// A missing parent transcript is the ordinary case for a session whose
    /// agent writes under a config dir TUIC cannot read. It must cost the spawn
    /// timing, never the map.
    #[test]
    fn subagent_map_build_tolerates_a_missing_parent_transcript() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, _) = session_tree(tmp.path(), "");
        assert_eq!(nodes(&dir, &tmp.path().join("gone.jsonl")).len(), 1);
    }

    /// The live half of the feature: a subagent spawned while the page is open
    /// must arrive as a new node on the next poll, and its counts must keep
    /// growing as it appends. A cached directory listing, or an offset that
    /// never revisits a file it has already read, both pass every other test.
    #[test]
    fn subagent_map_build_picks_up_a_lane_that_appears_between_polls() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), PARENT_JSONL);
        let mut cache = MapCache::default();
        let poll = |cache: &mut MapCache| subagent_nodes(cache, &dir, &parent, "s", NOW_MS);
        assert_eq!(poll(&mut cache).len(), 1);

        // A second subagent starts: its meta lands first, its transcript grows
        // afterwards — the order Claude writes them in.
        std::fs::write(dir.join("agent-alate.meta.json"), fixture("minimal")).expect("meta");
        let late = dir.join("agent-alate.jsonl");
        std::fs::write(&late, "").expect("empty transcript");

        let tree = poll(&mut cache);
        let node = tree
            .iter()
            .find(|n| n.id == "alate")
            .expect("the new node arrives");
        assert_eq!(node.started_at_ms, None, "it has written nothing yet");
        assert_eq!(node.state, NodeState::Running);

        append(&late, &format!("{}\n", row("2026-09-21T10:00:20Z", "Grep")));
        let tree = poll(&mut cache);
        let node = tree.iter().find(|n| n.id == "alate").expect("still there");
        assert_eq!(
            node.tools,
            vec![ToolCount {
                name: "Grep".into(),
                count: 1
            }]
        );
        assert_eq!(node.started_at_ms, Some(LANE_ORIGIN_MS + 20_000));
    }

    /// The whole point of the cache: a second poll over an unchanged tree
    /// re-reads nothing and still answers with the same map.
    #[test]
    fn subagent_map_build_is_stable_across_polls() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (dir, parent) = session_tree(tmp.path(), PARENT_JSONL);
        let mut cache = MapCache::default();
        let first = subagent_nodes(&mut cache, &dir, &parent, "s", NOW_MS);
        let second = subagent_nodes(&mut cache, &dir, &parent, "s", NOW_MS);
        assert_eq!(first, second);
    }

    /// Revised 2026-09-23 from `subagent_map_events_never_carry_a_prompt_or_a_result_body`.
    ///
    /// Scoped to the `/agents/map` tree. The Progress Flow view does carry the
    /// subagent's final report, redacted and cut; that contract is
    /// `flow_carries_a_redacted_report_but_never_a_tool_result` in
    /// `progress/flow.rs`.
    ///
    /// The old contract kept every prompt off the wire; the call map needs the
    /// prompt to say *why* a subagent exists. The new contract: a prompt reaches
    /// the payload only redacted and at most `PROMPT_SUMMARY_CHARS` long, and
    /// the full text only through the expand endpoint. Tool-result bodies and
    /// the subagent's own replies still never reach it.
    #[test]
    fn subagent_map_payload_carries_only_a_redacted_prompt_summary_and_never_a_result_body() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let secret = "sk-ant-api03-SECRETSECRETSECRETSECRET";
        let prompt = format!(
            "Deploy with {secret} then {}",
            "check the rollout. ".repeat(30)
        );
        let parent = serde_json::json!({"timestamp":"2026-09-21T09:59:58.000Z","message":{"content":[{"type":"tool_use","id":"toolu_FIXTURE_TEAMMATE","name":"Agent","input":{"prompt":prompt}}]}}).to_string();
        let (dir, transcript) = session_tree(tmp.path(), &format!("{parent}\n"));
        append(
            &dir.join("agent-alane-fixture.jsonl"),
            &format!(
                "{}\n{}\n",
                r#"{"timestamp":"2026-09-21T10:00:10Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t5","content":"RESULT-BODY-SENTINEL"}]}}"#,
                r#"{"timestamp":"2026-09-21T10:00:11Z","message":{"role":"assistant","content":[{"type":"text","text":"REPLY-BODY-SENTINEL"}]}}"#,
            ),
        );
        let first = std::fs::read_to_string(dir.join("agent-alane-fixture.jsonl")).unwrap();
        let first = first.replacen(
            "Review the diff and report",
            &prompt.replace('"', "\\\""),
            1,
        );
        std::fs::write(dir.join("agent-alane-fixture.jsonl"), first).unwrap();

        let tree = nodes(&dir, &transcript);
        let wire = serde_json::to_string(&tree).expect("serialises");
        assert!(
            !wire.contains(secret),
            "a secret reached the payload: {wire}"
        );
        assert!(
            !wire.contains("SECRETSECRET"),
            "part of a secret reached the payload"
        );
        assert!(
            !wire.contains("RESULT-BODY-SENTINEL"),
            "a tool result reached the payload"
        );
        assert!(
            !wire.contains("REPLY-BODY-SENTINEL"),
            "a reply body reached the payload"
        );
        let summary = tree[0].prompt_summary.as_deref().expect("a summary");
        assert!(summary.contains("[REDACTED]"), "{summary}");
        assert!(summary.chars().count() <= PROMPT_SUMMARY_CHARS);
        assert!(tree[0].prompt_more, "the prompt is longer than its summary");
    }

    /// Redaction runs on the whole prompt before the cut. A secret straddling
    /// the 200th character would otherwise lose its tail, stop matching its
    /// pattern, and ship its prefix.
    #[test]
    fn subagent_map_prompt_summary_redacts_before_it_truncates() {
        let pad = "x ".repeat((PROMPT_SUMMARY_CHARS - 6) / 2);
        let prompt = format!("{pad}ghp_{}", "A".repeat(40));
        let (summary, more) = prompt_summary(&prompt);
        let summary = summary.expect("summary");
        assert!(
            !summary.contains("ghp_"),
            "a token prefix survived: {summary}"
        );
        assert!(summary.chars().count() <= PROMPT_SUMMARY_CHARS);
        assert!(more);
        assert_eq!(prompt_summary(" \n\t"), (None, false), "blank is no prompt");
    }

    /// The expand endpoint returns the whole prompt, redacted, and only for an
    /// agent id found on disk. An id shaped like a path finds nothing, because
    /// it is compared against the listing and never joined into a path.
    #[test]
    fn subagent_map_prompt_is_full_redacted_and_looked_up_by_listing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let long = format!(
            "Use ghp_{} and {}",
            "B".repeat(40),
            "then keep going. ".repeat(40)
        );
        let parent = serde_json::json!({"timestamp":"2026-09-21T09:59:58.000Z","message":{"content":[{"type":"tool_use","id":"toolu_FIXTURE_TEAMMATE","name":"Agent","input":{"prompt":long}}]}}).to_string();
        let (dir, transcript) = session_tree(tmp.path(), &format!("{parent}\n"));
        let lane = std::fs::read_to_string(dir.join("agent-alane-fixture.jsonl")).unwrap();
        std::fs::write(
            dir.join("agent-alane-fixture.jsonl"),
            lane.replacen("Review the diff and report", &long, 1),
        )
        .unwrap();
        let mut cache = MapCache::default();

        let full = prompt_text(&mut cache, &dir, &transcript, "alane-fixture").expect("found");
        assert!(
            full.chars().count() > PROMPT_SUMMARY_CHARS,
            "not the summary"
        );
        assert!(
            full.contains("[REDACTED]") && !full.contains("ghp_"),
            "{full}"
        );
        assert!(
            full.contains('\n') || full.contains("then keep going."),
            "the body is intact"
        );

        assert_eq!(prompt_text(&mut cache, &dir, &transcript, "nope"), None);
        assert_eq!(
            prompt_text(
                &mut cache,
                &dir,
                &transcript,
                "../subagents/agent-alane-fixture"
            ),
            None
        );
    }

    fn prompt_text(
        cache: &mut MapCache,
        dir: &Path,
        transcript: &Path,
        agent_id: &str,
    ) -> Option<String> {
        subagent_text(cache, dir, transcript, agent_id, TextPart::Prompt)
    }

    fn terminal(id: &str, title: &str, parent: Option<&str>) -> TerminalInfo {
        TerminalInfo {
            session_id: id.into(),
            title: title.into(),
            agent_type: Some("claude".into()),
            intent: Some("Refactor the map".into()),
            state: NodeState::Busy,
            parent_session: parent.map(Into::into),
            subagents: Vec::new(),
        }
    }

    fn sub(id: &str, state: NodeState) -> MapNode {
        MapNode {
            kind: NodeKind::Subagent,
            id: id.into(),
            session_id: "t".into(),
            title: id.into(),
            agent_type: None,
            model: None,
            description: None,
            intent: None,
            state,
            started_at_ms: None,
            duration_ms: None,
            prompt_summary: None,
            prompt_more: false,
            tools: Vec::new(),
            other_tools: 0,
            tool_calls: 0,
            children: Vec::new(),
        }
    }

    /// A peer spawned with `agent action=spawn` hangs under the terminal that
    /// spawned it, after that terminal's own subagents. A parent that is not a
    /// live terminal leaves the child a root.
    #[test]
    fn subagent_map_assemble_nests_peers_under_their_spawner() {
        let mut lead = terminal("lead", "orchestrator", None);
        lead.subagents = vec![sub("a1", NodeState::Running)];
        let map = assemble(vec![
            terminal("w1", "worker one", Some("lead")),
            lead,
            terminal("w2", "worker two", Some("gone")),
        ]);

        let roots: Vec<&str> = map.roots.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(roots, vec!["lead", "w2"], "sorted by title; w1 is nested");
        let lead = &map.roots[0];
        assert_eq!(lead.intent.as_deref(), Some("Refactor the map"));
        assert_eq!(lead.agent_type.as_deref(), Some("claude"));
        let kids: Vec<&str> = lead.children.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            kids,
            vec!["a1", "w1"],
            "own subagents first, then spawned peers"
        );
        assert_eq!(map.terminals, 3);
        assert_eq!(map.subagents, 1);
        assert_eq!(map.running, 1);
    }

    /// Spawn links come from runtime state, not from a structure that
    /// guarantees a tree. A cycle must cost the edge, never the nodes — and a
    /// node hanging off a cycle keeps its own edge.
    #[test]
    fn subagent_map_assemble_survives_a_parent_cycle() {
        let map = assemble(vec![
            terminal("a", "a", Some("b")),
            terminal("b", "b", Some("a")),
            terminal("c", "c", Some("a")),
            terminal("self", "self", Some("self")),
        ]);
        let roots: Vec<&str> = map.roots.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(roots, vec!["a", "b", "self"]);
        assert_eq!(map.roots[0].children.len(), 1);
        assert_eq!(map.roots[0].children[0].id, "c");
        assert_eq!(map.terminals, 4, "no node is lost");
    }

    /// An empty intent is no intent, and a terminal titled by its agent type
    /// does not print the type twice.
    #[test]
    fn subagent_map_assemble_drops_repeated_and_empty_text() {
        let mut t = terminal("x", "claude", None);
        t.intent = Some("  ".into());
        let map = assemble(vec![t]);
        assert_eq!(map.roots[0].intent, None);
        assert_eq!(map.roots[0].agent_type, None);
    }
}
