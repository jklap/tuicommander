# Transport Layer

The transport layer provides a unified IPC abstraction so the same frontend code works in both Tauri (native desktop) and browser (HTTP) modes.

## Files

| File | Purpose |
|------|---------|
| `src/invoke.ts` | Smart `invoke()` wrapper — zero overhead in Tauri |
| `src/transport.ts` | HTTP transport implementation and command-to-endpoint mapping |

## invoke.ts

```typescript
export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>
export function listen<T>(event: string, handler: (event: Event<T>) => void): Promise<Unsubscribe>
```

**Resolution:** At module import time, detects if running in Tauri webview:
- **Tauri mode:** Delegates directly to `@tauri-apps/api/core.invoke()` (zero overhead)
- **Browser mode:** Maps command to HTTP endpoint via `transport.ts`

### In-flight dedup (Tauri mode)

Concurrent identical calls for read-only commands in `DEDUP_COMMANDS` share a single IPC round-trip — the second caller gets the same `Promise` as the first, cleared on settle. Prevents the `repo-changed` fan-out storm where ~20 mounted components each spawned parallel git processes for the same repo. Mutations (stage/commit/push) are never deduped. Browser mode has the equivalent via `isIdempotentRpc` in `transport.ts`.

```typescript
export function isTauri(): boolean
// Checks window.__TAURI__ existence
```

## transport.ts

### Command Mapping

Maps every Tauri command name to an HTTP method + path via a declarative `COMMAND_TABLE`. Each entry is a `CommandTableEntry` — an object `{ map }` whose `map(args, p)` returns `{ method, path, body?, transform? }`.

```typescript
// Table-driven: each command maps to an HTTP request
const COMMAND_TABLE: Record<string, CommandTableEntry> = {
  create_pty: { map: (args) => ({ method: "POST", path: "/sessions", body: args.config }) },
  get_repo_info: { map: (args) => ({ method: "GET", path: `/repo/info?path=${enc(args.path)}` }) },
  write_pty: { map: (args) => ({ method: "POST", path: `/sessions/${args.sessionId}/write`, body: { data: args.data } }) },
  // ... ~80 commands
};
```

This replaces the previous 370-line switch statement with a flat lookup table for easier maintenance and review.

### PTY Subscription

```typescript
export function subscribePty(
  sessionId: string,
  onData: (data: string) => void,
  onExit: () => void,
  onParsedOrOptions?: ((event: WsParsedEvent) => void) | SubscribePtyOptions
): Unsubscribe
```

- **Tauri mode:** Uses `listen("pty-activity-{id}")` and `listen("pty-exit-{id}")` Tauri events
- **Browser mode:** Opens WebSocket to `/sessions/{id}/stream`

`onData` receives PTY output in **browser/PWA mode only**. Desktop sends no output
over IPC: the canvas renders from grid frames and plugin watcher lines are
assembled in Rust, so no desktop consumer needs the bytes.

For "is this session producing output", use `options.onActivity` — a payload-free
pulse the backend throttles to ~1/s and delivers on both transports from one
signal (`pty-activity-{id}` on desktop, the `{"type":"activity"}` WS frame in the
browser). It is the only activity signal that works for a background tab: the
canvas stops acking grid frames while a terminal is hidden, so frame traffic
cannot stand in for it.

### URL Building

```typescript
export function buildHttpUrl(path: string): string
// Reads MCP port from config, builds http://localhost:{port}{path}
```

## Design

The transport abstraction enables:

1. **Development:** Run frontend with `pnpm dev` against the Rust HTTP server
2. **Browser mode:** Access TUICommander from a browser on another device
3. **Testing:** Frontend tests can mock at the invoke level
4. **MCP integration:** External tools use the same HTTP API

The abstraction is resolved once at module load — no per-call overhead in production Tauri mode.
