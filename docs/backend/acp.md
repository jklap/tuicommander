# ACP Client for ego

TUICommander speaks the Agent Client Protocol (v1) as a **client**, to
[ego](https://github.com/sstraus/ego) as the agent. Two repositories, no shared
crate: both pin `agent-client-protocol` and agree on the wire, not on types.

Backend only so far. There is no frontend surface; everything below is reachable
from `acp_*` Tauri commands and, identically, from `/acp` HTTP routes.

## Layout

| File | What lives there |
|------|------------------|
| `src-tauri/src/acp/mod.rs` | The vocabulary: ids, snapshots, events, errors, notices, and the capability snapshot taken at `initialize` |
| `src-tauri/src/acp/manager.rs` | `AcpClientManager` — one supervised child per connection, and every operation a caller can ask for |
| `src-tauri/src/acp/connection.rs` | The per-connection actor: decides and writes serially, waits concurrently |
| `src-tauri/src/acp/events.rs` | `AcpEventJournal` — the ordered, bounded record every subscriber reads from |
| `src-tauri/src/acp/ego_ext.rs` | `_ego/pause`, `_ego/resume`, `_ego/compact` |
| `src-tauri/src/acp_commands.rs` | The Tauri surface: one command per manager method, nothing else |
| `src-tauri/src/mcp_http/acp_routes.rs` | The HTTP surface, calling the same cores |

## The actor

One `ConnectionActor` per connection. Deciding and writing is serial — a single
task owns the decision of whether an operation is allowed and the write that
follows, so two callers cannot both pass a gate that only one of them should
have. Waiting is concurrent: replies are awaited in a `FuturesUnordered`, so a
turn that takes a minute does not hold up a cancel.

Agent→client requests (`session/request_permission`,
`session/create_elicitation`) arrive on the **same** channel as `session/update`.
That is deliberate: the question and the updates that explain what is being
asked about must stay in the order the agent sent them, and two channels would
reorder them.

The SDK hands a responder to a callback that holds the dispatch loop. Answering
there would freeze the connection for as long as a person takes to decide — so
the responder is carried out of the callback and parked. What the tests pin is
that it is always answered exactly once: by the person, by the cancel that took
the question away, or immediately when there was no seat to offer.

**Capabilities are checked on the way in too.** `initialize` advertises
`elicitation.form` and nothing else, so an `elicitation/create` in `url` mode —
or in one from a version this client has never seen — is declined on arrival
rather than seated. Seating is how rendering happens here: the request lands on
the attachment and a frontend draws the form it knows how to draw, which is
precisely what the protocol says a client must not do with a mode it does not
understand. A URL elicitation drawn as a form is worse than a refusal, because
the address it is actually asking about is what a form has nowhere to put.

**A bad answer never costs the person their question.** An answer is validated
before the seat is taken, so a permission option the agent never offered, or an
elicitation action outside the protocol's three, is refused with `invalid_input`
and the question stays open for a real answer. Forwarding either would spend the
seat on something the agent cannot read, and nobody could then try again.

## The journal

Every accepted callback and every local settlement is stamped with a sequence
number, appended, and only then fanned out. That gives a host two things a
snapshot cannot: the order the agent said things in, and the ability to join
late.

It is bounded (1024 events). A subscriber asking for a cursor that has fallen
off the end is told `stream_gap` rather than being resumed past the hole: the
missing chunks exist nowhere in this client, and a stream that silently skipped
them would render as a turn where part of what the model said never happened.
Recovery is a fresh connection and `session/load`, which replays from ego — the
only place that still knows.

Nothing in `append` can block the SDK reader: an uncontended lock and a
broadcast send that drops the slowest subscriber's view rather than waiting for
it.

## Capabilities are decided before the wire

The `initialize` response becomes an immutable capability snapshot. Every
operation is checked against it, and an unadvertised one is refused with
`capability_unavailable` **without sending a byte**. The same holds for content
blocks a session never said it accepts, for extra roots, and for ego's own
extensions — whose advertised version must match the one this client speaks.

This is why `capability_unavailable` is never retryable: the answer comes from a
snapshot taken once, so the identical request on this connection will refuse
identically. Reaching an agent that has it is a different action, not a retry.

There is no provider surface, and that is the same rule rather than a gap.
`providers/list|set|disable` are a draft the pinned schema keeps behind an
unstable feature, so ego does not serve them and never advertises them — and
`providers/set` would let whatever is connected rewrite the base URL that every
generation egresses to. Model, effort and mode arrive instead as
`session/set_config_option` options the session itself publishes, so a host
picks from what the agent offered rather than from a list this client made up.

The snapshot is also what makes `method_not_found` fatal. Nothing reaches the
wire that the snapshot did not allow, so a method-not-found coming back is not
the agent declining — it is the agent disowning what it published, and the one
thing this client knows about it is no longer true. The caller is answered with
`protocol_violation`, and only then does the connection settle as failed: a host
that asked deserves to be told why, not just that the connection is gone.

## Two transports, one set of judgements

`acp_commands.rs` and `acp_routes.rs` are both thin. Neither decides what is
allowed, validates a permission option, or invents a fallback — those are the
client's judgements, made once in `crate::acp`, so the desktop and a phone
cannot disagree.

The error body is the serialized `AcpClientError` on both. Over HTTP the status
is a translation of its `code`, never a second opinion: 400/404 for the caller,
501 when the agent never advertised the operation, 410 when the connection or
the cursor is genuinely gone, 502 when the agent answered with a refusal.

The one asymmetry is the stream, and it is a transport detail: a Tauri Channel
on the desktop, a WebSocket in the browser, carrying byte-identical frames.

## Lifecycle invariants

Written down because each of them is a variant the plan named and this client
deliberately does not have:

- **A connection id is not public until initialization has succeeded.** It is
  minted inside `connect` and does not escape until v1 is negotiated, so there
  is no `starting` or `initializing` state — nothing that is still coming up can
  be named, listed, subscribed to, or asked about. Launch and negotiation are
  phases of the `connect` operation, not states of a connection.
- **A failed attempt is never registered and never settles.** It is answered
  with `initialization_failed` as an *error code*; there is no settlement reason
  by that name, because there is nothing there to settle.
- **A settled connection stays readable, and then is forgotten.** Ending is
  exactly when a host wants to read one — the reason, the attachments it had,
  the tail of its stream — so the last eight are kept. Kept forever they would
  be a leak: each holds a journal of up to a thousand events, and a `reconnect`
  loop against an agent that keeps dying is an ordinary thing to happen.
- **`transport_error` covers write failures too.** A child that closes its pipes
  or exits produces a write failure, a stdout EOF and an SDK shutdown at once,
  and which one the supervisor sees first is a scheduling accident. A separate
  `write_error` would be an attribution no one here can make.
- **A line that is not a frame does not end the connection.** The SDK owns the
  framing: an unparseable line is answered with JSON-RPC's `-32700` and the
  reader carries on, and nothing about it reaches this client. So the mistake is
  reported to the only party that can do anything about it — the agent that made
  it — and the stream stays in sync, because a line is a frame boundary and the
  next one starts clean. The exception is a malformed line shaped like a
  *response*, which the SDK drops in silence: answering an answer is not a thing
  JSON-RPC can do, so a request it might have been for waits out the connection.

- **A session is attached to a connection at most once.** A `load` or `resume`
  naming a session this connection already holds is refused with
  `invalid_input`, not merged and not re-run. The attachment is where the
  running turn, the usage totals and the ids of the questions a person has open
  live, and attaching writes a fresh one — so a second attach would blank the
  turn, and the response that settles it would arrive for a turn nothing names
  any more and be dropped as stale, leaving a host watching a turn that never
  ends. The seats, meanwhile, outlive the overwrite, so the two views would then
  disagree about what is being asked. A host that wants the history replayed
  detaches first, which says what it means. A fork is not affected: it names the
  session it forks *from* and comes back with an id of its own.

Changing the first two means a different, asynchronous connect API — one that
hands back an id before there is a connection behind it, with its own
cancellation, retention and ownership rules. That is a decision to make when
something asks for it, not a set of enum variants to leave lying around.

## Process authority

The ego binary is the `ego_executable` setting, read at each `connect`. It is
not held by the manager — a copy taken once would keep launching the previous
binary after somebody corrected the setting, without ever saying so — and it is
not an argument of any command or route, so no request body can name what this
machine runs. An empty setting refuses every connect, because "not configured"
and "configured wrongly" are different things to be told.

`POST /acp/connections` and `.../reconnect` are the only routes that launch a
process, and the only two that take the loopback-or-authenticated guard.

## Notices

`AcpEventJournal::append` is the single place every event is stamped, so it is
where the wake signal is derived. Four kinds — `ready`, `settled`,
`interaction_pending`, `interaction_settled` — become an `AppEvent::AcpNotice`
that `spawn_acp_notice_pump` mirrors to the desktop window and to `/events`.

A turn's chunks never ride that bus: one turn emits more of them per second than
the 256-entry broadcast can carry without lagging every unrelated subscriber in
the app. A notice says where to look; the payload stays on the stream, the
snapshot, or the interactions list.

`settled` covers both a turn finishing and a connection ending. They are told
apart by whether the notice names a session, not by a fifth kind.

## Tests

`src-tauri/tests/story092_*.rs`, against `tuic-acp-fixture-agent` — a
non-bundled binary that replays a `tests/fixtures/acp/*.jsonl` scenario through
the exact production launch path. `tests/fixtures/acp/ego-initialize.json` is a
recording of what real ego answers, copied from ego's own committed golden;
editing it to make a test pass would turn the recording into a wish.
