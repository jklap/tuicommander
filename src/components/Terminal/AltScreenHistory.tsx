import { Component, For, Index, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { type LogLine, spanStyle, lineText } from "../../mobile/utils/logLine";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { settingsStore } from "../../stores/settings";
import { trimSelection } from "./Terminal";
import { SearchBar } from "../shared/SearchBar";
import type { SearchOptions } from "../shared/DomSearchEngine";
import { searchLogLines, highlightSpans, type SearchMatch } from "./scrollbackSearch";
import s from "./AltScreenHistory.module.css";

const POLL_INTERVAL = 500;

function deduplicatedScreen(log: LogLine[], screen: LogLine[]): LogLine[] {
  if (screen.length === 0 || log.length === 0) return screen;
  const lastLogTexts = log.slice(-screen.length).map(lineText);
  let overlap = 0;
  for (let start = 0; start <= lastLogTexts.length - screen.length; start++) {
    let match = true;
    for (let j = 0; j < screen.length && start + j < lastLogTexts.length; j++) {
      if (lastLogTexts[start + j] !== lineText(screen[j])) {
        match = false;
        break;
      }
    }
    if (match) {
      overlap = Math.min(screen.length, lastLogTexts.length - start);
      break;
    }
  }
  return overlap > 0 ? screen.slice(overlap) : screen;
}

interface VtLogChunk {
  lines: LogLine[];
  screen: LogLine[];
  total_lines: number;
  oldest: number;
}

interface Props {
  sessionId: string;
  onClose: () => void;
  terminalBg: string;
  fontFamily: string;
  fontSize: number;
  fontWeight: string;
  cellHeight: number;
  cellWidth: number;
  cols: number;
  searchVisible: boolean;
  onSearchClose: () => void;
}

export const AltScreenHistory: Component<Props> = (props) => {
  let containerEl: HTMLDivElement | undefined;
  let ignoreScrollUntil = 0;

  const [logLines, setLogLines] = createSignal<LogLine[]>([]);
  const [screenRows, setScreenRows] = createSignal<LogLine[]>([]);
  let newestTotal = 0;

  async function fetchAll() {
    try {
      const chunk = await invoke<VtLogChunk>("read_vt_log", {
        sessionId: props.sessionId,
        offset: 0,
        limit: 100000,
      });
      setLogLines(chunk.lines);
      setScreenRows(chunk.screen);
      newestTotal = chunk.total_lines;
    } catch (err) {
      appLogger.error("terminal", "read_vt_log failed", { error: String(err) });
    }
  }

  async function fetchNewer() {
    try {
      const chunk = await invoke<VtLogChunk>("read_vt_log", {
        sessionId: props.sessionId,
        offset: newestTotal,
        limit: 500,
      });
      if (chunk.lines.length > 0) {
        setLogLines((prev) => [...prev, ...chunk.lines]);
      }
      setScreenRows(chunk.screen);
      newestTotal = chunk.total_lines;
    } catch (err) {
      appLogger.debug("terminal", "read_vt_log poll failed", { sessionId: props.sessionId, error: String(err) });
    }
  }

  onMount(() => {
    fetchAll().then(() => {
      requestAnimationFrame(() => {
        if (containerEl) {
          ignoreScrollUntil = performance.now() + 150;
          containerEl.scrollTop = containerEl.scrollHeight;
        }
      });
    });

    const pollId = setInterval(fetchNewer, POLL_INTERVAL);
    onCleanup(() => clearInterval(pollId));
  });

  const handleScroll = () => {
    if (!containerEl || performance.now() < ignoreScrollUntil) return;
    const canScroll = containerEl.scrollHeight > containerEl.clientHeight + 16;
    if (!canScroll) return;
    const atBottom =
      containerEl.scrollTop + containerEl.clientHeight >= containerEl.scrollHeight - 8;
    if (atBottom) props.onClose();
  };

  const handleMouseUp = () => {
    if (!settingsStore.state.copyOnSelect) return;
    const sel = window.getSelection()?.toString();
    if (!sel || sel.length < 2) return;
    const trimmed = trimSelection(sel);
    if (!trimmed) return;
    const setStatus = (window as unknown as Record<string, unknown>).__tuic_setStatusInfo as ((msg: string) => void) | undefined;
    navigator.clipboard.writeText(trimmed).then(() => {
      setStatus?.("Copied to clipboard");
    }).catch((err) => {
      appLogger.warn("terminal", "Scroll history copy-on-select failed", err);
    });
  };

  const dedupScreen = createMemo(() => deduplicatedScreen(logLines(), screenRows()));
  const allLines = createMemo(() => [...logLines(), ...dedupScreen()]);

  const [matches, setMatches] = createSignal<SearchMatch[]>([]);
  const [activeIdx, setActiveIdx] = createSignal(-1);
  let searchToken = 0;

  const handleSearch = (term: string, opts: SearchOptions) => {
    const token = ++searchToken;
    const found = searchLogLines(allLines(), term, opts);
    if (token !== searchToken) return;
    setMatches(found);
    setActiveIdx(found.length > 0 ? 0 : -1);
    if (found.length > 0) scrollToMatch(0);
  };

  const handleNext = () => {
    const m = matches();
    if (m.length === 0) return;
    const next = (activeIdx() + 1) % m.length;
    setActiveIdx(next);
    scrollToMatch(next);
  };

  const handlePrev = () => {
    const m = matches();
    if (m.length === 0) return;
    const prev = (activeIdx() - 1 + m.length) % m.length;
    setActiveIdx(prev);
    scrollToMatch(prev);
  };

  function scrollToMatch(idx: number) {
    const m = matches()[idx];
    if (!m || !containerEl) return;
    const rows = containerEl.querySelectorAll(`.${s.row}`);
    const row = rows[m.lineIndex];
    if (row) {
      ignoreScrollUntil = performance.now() + 150;
      row.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  }

  const matchesByLine = createMemo(() => {
    const m = matches();
    const map = new Map<number, { matches: SearchMatch[]; globalOffset: number }>();
    for (let i = 0; i < m.length; ) {
      const lineIdx = m[i].lineIndex;
      const start = i;
      while (i < m.length && m[i].lineIndex === lineIdx) i++;
      map.set(lineIdx, { matches: m.slice(start, i), globalOffset: start });
    }
    return map;
  });

  const renderLine = (line: LogLine, lineIndex: number) => {
    const info = matchesByLine().get(lineIndex);
    if (!info) {
      return (
        <div class={s.row}>
          <Index each={line.spans}>
            {(span) => {
              const st = spanStyle(span());
              return st
                ? <span style={st}>{span().text}</span>
                : <>{span().text}</>;
            }}
          </Index>
        </div>
      );
    }
    const segs = highlightSpans(line, info.matches, activeIdx(), info.globalOffset);
    return (
      <div class={s.row}>
        <For each={segs}>{(seg) => {
          const st = spanStyle(seg.span);
          const cls = seg.highlight
            ? seg.active ? s.matchActive : s.matchHighlight
            : undefined;
          return st || cls
            ? <span style={st || {}} class={cls}>{seg.text}</span>
            : <>{seg.text}</>;
        }}</For>
      </div>
    );
  };

  return (
    <div
      ref={containerEl}
      class={s.overlay}
      style={{ background: props.terminalBg }}
      onScroll={handleScroll}
      onMouseUp={handleMouseUp}
    >
      <SearchBar
        visible={props.searchVisible}
        onSearch={handleSearch}
        onNext={handleNext}
        onPrev={handlePrev}
        onClose={props.onSearchClose}
        matchIndex={activeIdx()}
        matchCount={matches().length}
        matchLabel={matches().length >= 1000 ? "1000+" : undefined}
      />
      <div class={s.header} style={{ background: props.terminalBg }}>
        <span class={s.label}>Scroll history — {logLines().length} lines</span>
        <button class={s.closeBtn} onClick={props.onClose}>
          Return to live ↓
        </button>
      </div>
      <div
        class={s.content}
        style={{
          "--cell-height": `${props.cellHeight}px`,
          "--content-width": `${props.cols * props.cellWidth}px`,
          "font-family": props.fontFamily,
          "font-size": `${props.fontSize}px`,
          "font-weight": props.fontWeight,
          "letter-spacing": `calc(${props.cellWidth}px - 1ch)`,
        }}
      >
        <For each={allLines()}>{(line, i) => renderLine(line, i())}</For>
      </div>
    </div>
  );
};
