import { Component, createSignal, onMount, onCleanup } from "solid-js";
import { settingsStore } from "../../stores/settings";
import {
  decodeBinaryFrame,
  measureFont,
  computeCursorRect,
  type CellMetrics,
  type CursorShape,
  type DecodedFrame,
  type DecodedCell,
} from "./canvasTerminalUtils";
import { keyToSequence } from "./terminalInput";
import { isSuggestBlock, continuationRowsAfterSuggest } from "./suggestOverlay";
import {
  filePathRegex,
  fileUrlRegex,
} from "./linkProvider";
import { terminalsStore } from "../../stores/terminals";
// Re-export for external consumers
export type { CellMetrics, CursorShape, DecodedFrame, DecodedCell };

export interface CanvasTerminalProps {
  sessionId: string;
  onOpenFilePath?: (path: string, line?: number, col?: number) => void;
}

const SUGGEST_ANCHOR_RE = /^[\s●⏺]*suggest:\s+\S/;
const INTENT_RE = /^[\s●⏺]*intent:\s+/;

const CanvasTerminal: Component<CanvasTerminalProps> = (props) => {
  let canvasRef!: HTMLCanvasElement;
  let scrollbarRef!: HTMLDivElement;
  let scrollThumbRef!: HTMLDivElement;
  let overlayRef!: HTMLDivElement;
  let containerRef!: HTMLDivElement;
  let ctx: CanvasRenderingContext2D;

  const [metrics, setMetrics] = createSignal<CellMetrics | null>(null);
  const [focused, setFocused] = createSignal(false);
  let currentFrame: DecodedFrame | null = null;
  let cursorShape: CursorShape = "block";
  let cursorBlinkOn = true;
  let blinkInterval: ReturnType<typeof setInterval> | undefined;
  let unsubscribe: (() => void) | undefined;
  let resizeObserver: ResizeObserver | undefined;
  let invokeRef: ((cmd: string, args: Record<string, unknown>) => Promise<unknown>) | undefined;

  // Selection state
  let selecting = false;
  let selectionStart: { col: number; row: number } | null = null;
  let selectionEnd: { col: number; row: number } | null = null;

  // Link detection
  const linkCache = new Map<string, { text: string; path: string; line?: number; col?: number; index: number }[] | null>();
  let hoveredLink: { row: number; colStart: number; colEnd: number; path: string; line?: number; col?: number } | null = null;

  function writePty(data: string) {
    invokeRef?.("write_pty", { sessionId: props.sessionId, data }).catch(() => {});
  }

  function canvasToGrid(e: MouseEvent): { col: number; row: number } {
    const m = metrics();
    if (!m) return { col: 0, row: 0 };
    const rect = canvasRef.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    return {
      col: Math.floor(x / m.cellWidth),
      row: Math.floor(y / m.cellHeight),
    };
  }

  function remeasure() {
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const fontSize = settingsStore.state.defaultFontSize;
    const fontFamily = settingsStore.getFontFamily();
    const m = measureFont(ctx, fontSize, fontFamily, dpr);
    setMetrics(m);

    const rect = canvasRef.getBoundingClientRect();
    canvasRef.width = Math.floor(rect.width * dpr);
    canvasRef.height = Math.floor(rect.height * dpr);
    ctx.scale(dpr, dpr);

    if (currentFrame) {
      paintFrame(currentFrame, m);
    }
  }

  function paintFrame(frame: DecodedFrame, m: CellMetrics) {
    const fontFamily = settingsStore.getFontFamily();
    const bgDefault = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
    const fgDefault = getComputedStyle(canvasRef).getPropertyValue("--text-primary").trim() || "#d4d4d4";

    for (const row of frame.rows) {
      const y = row.index * m.cellHeight;
      ctx.clearRect(0, y, canvasRef.width / m.dpr, m.cellHeight);

      for (let c = 0; c < row.cells.length; c++) {
        const cell = row.cells[c];
        if (cell.char === "") continue;

        const x = c * m.cellWidth;
        const fg = resolveFg(cell, fgDefault);
        const bg = resolveBg(cell, bgDefault);

        if (!cell.defaultBg || cell.inverse) {
          ctx.fillStyle = bg;
          ctx.fillRect(x, y, m.cellWidth, m.cellHeight);
        }

        if (cell.char !== " ") {
          const fontStyle = buildFontStyle(cell, m.fontSize, fontFamily);
          ctx.font = fontStyle;
          ctx.fillStyle = fg;
          if (cell.dim) ctx.globalAlpha = 0.5;
          ctx.fillText(cell.char, x, y + m.baseline);
          if (cell.dim) ctx.globalAlpha = 1.0;
        }

        if (cell.underline) {
          ctx.fillStyle = fg;
          ctx.fillRect(x, y + m.cellHeight - 1, m.cellWidth, 1);
        }

        if (cell.strikeout) {
          ctx.fillStyle = fg;
          ctx.fillRect(x, y + Math.floor(m.cellHeight / 2), m.cellWidth, 1);
        }
      }
    }

    // Selection overlay
    paintSelection(frame, m);
    paintCursor(frame, m);
    updateScrollbar(frame);
    updateSuggestOverlay(frame, m);
  }

  function paintSelection(frame: DecodedFrame, m: CellMetrics) {
    if (!selectionStart || !selectionEnd) return;
    const startRow = Math.min(selectionStart.row, selectionEnd.row);
    const endRow = Math.max(selectionStart.row, selectionEnd.row);

    ctx.fillStyle = "rgba(58, 130, 220, 0.35)";

    for (const row of frame.rows) {
      if (row.index < startRow || row.index > endRow) continue;
      const y = row.index * m.cellHeight;

      if (startRow === endRow) {
        const c0 = Math.min(selectionStart.col, selectionEnd.col);
        const c1 = Math.max(selectionStart.col, selectionEnd.col);
        ctx.fillRect(c0 * m.cellWidth, y, (c1 - c0 + 1) * m.cellWidth, m.cellHeight);
      } else if (row.index === startRow) {
        const isStartFirst = selectionStart.row <= selectionEnd.row;
        const startCol = isStartFirst ? selectionStart.col : selectionEnd.col;
        ctx.fillRect(startCol * m.cellWidth, y, (row.cells.length - startCol) * m.cellWidth, m.cellHeight);
      } else if (row.index === endRow) {
        const isStartFirst = selectionStart.row <= selectionEnd.row;
        const endCol = isStartFirst ? selectionEnd.col : selectionStart.col;
        ctx.fillRect(0, y, (endCol + 1) * m.cellWidth, m.cellHeight);
      } else {
        ctx.fillRect(0, y, row.cells.length * m.cellWidth, m.cellHeight);
      }
    }
  }

  function resolveFg(cell: DecodedCell, defaultColor: string): string {
    if (cell.inverse) {
      return cell.defaultBg ? defaultColor : `rgb(${cell.bgR},${cell.bgG},${cell.bgB})`;
    }
    return cell.defaultFg ? defaultColor : `rgb(${cell.fgR},${cell.fgG},${cell.fgB})`;
  }

  function resolveBg(cell: DecodedCell, defaultColor: string): string {
    if (cell.inverse) {
      return cell.defaultFg ? defaultColor : `rgb(${cell.fgR},${cell.fgG},${cell.fgB})`;
    }
    return cell.defaultBg ? defaultColor : `rgb(${cell.bgR},${cell.bgG},${cell.bgB})`;
  }

  function buildFontStyle(cell: DecodedCell, fontSize: number, fontFamily: string): string {
    let style = "";
    if (cell.italic) style += "italic ";
    if (cell.bold) style += "bold ";
    return `${style}${fontSize}px ${fontFamily}`;
  }

  function paintCursor(frame: DecodedFrame, m: CellMetrics) {
    if (!frame.cursorVisible) return;
    if (!cursorBlinkOn && focused()) return;

    const fgDefault = getComputedStyle(canvasRef).getPropertyValue("--text-primary").trim() || "#d4d4d4";
    const rect = computeCursorRect(cursorShape, frame.cursorRow, frame.cursorCol, m);

    if (!focused()) {
      ctx.strokeStyle = fgDefault;
      ctx.lineWidth = 1;
      ctx.strokeRect(rect.x + 0.5, rect.y + 0.5, rect.w - 1, rect.h - 1);
      return;
    }

    ctx.fillStyle = fgDefault;
    ctx.fillRect(rect.x, rect.y, rect.w, rect.h);

    if (cursorShape === "block") {
      const row = frame.rows.find((r) => r.index === frame.cursorRow);
      const cell = row?.cells[frame.cursorCol];
      if (cell && cell.char && cell.char !== " ") {
        const bgDefault = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
        const fontFamily = settingsStore.getFontFamily();
        ctx.font = buildFontStyle(cell, m.fontSize, fontFamily);
        ctx.fillStyle = bgDefault;
        ctx.fillText(cell.char, rect.x, frame.cursorRow * m.cellHeight + m.baseline);
      }
    }
  }

  // --- Scrollbar ---

  function updateScrollbar(frame: DecodedFrame) {
    if (!scrollbarRef || !scrollThumbRef) return;
    const total = frame.historySize + (frame.rows.length > 0 ? Math.max(...frame.rows.map(r => r.index)) + 1 : 24);
    const visible = canvasRef.getBoundingClientRect().height / (metrics()?.cellHeight ?? 16);

    if (frame.historySize === 0) {
      scrollbarRef.style.display = "none";
      return;
    }
    scrollbarRef.style.display = "block";

    const thumbRatio = Math.min(1, visible / total);
    const thumbHeight = Math.max(20, scrollbarRef.clientHeight * thumbRatio);
    const scrollRange = scrollbarRef.clientHeight - thumbHeight;
    const scrollPos = frame.historySize > 0
      ? (1 - frame.displayOffset / frame.historySize) * scrollRange
      : scrollRange;

    scrollThumbRef.style.height = `${thumbHeight}px`;
    scrollThumbRef.style.transform = `translateY(${scrollPos}px)`;
  }

  // --- Suggest / Intent overlay ---

  function updateSuggestOverlay(frame: DecodedFrame, m: CellMetrics) {
    if (!overlayRef) return;
    const bg = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
    const numRows = frame.rows.length > 0 ? Math.max(...frame.rows.map(r => r.index)) + 1 : 0;

    const getRowSnapshot = (i: number) => {
      const row = frame.rows.find(r => r.index === i);
      if (!row) return null;
      const text = row.cells.map(c => c.char || " ").join("");
      return { text, isWrapped: false };
    };

    let html = "";
    for (let row = 0; row < numRows; row++) {
      const snapshot = getRowSnapshot(row);
      if (!snapshot) continue;
      const text = snapshot.text;

      if (SUGGEST_ANCHOR_RE.test(text) && isSuggestBlock(row, numRows, getRowSnapshot)) {
        const top = row * m.cellHeight;
        html += `<div style="position:absolute;left:0;right:0;top:${top}px;height:${m.cellHeight}px;background:${bg}"></div>`;
        const hiddenRows = continuationRowsAfterSuggest(row, numRows, getRowSnapshot);
        for (const contRow of hiddenRows) {
          html += `<div style="position:absolute;left:0;right:0;top:${contRow * m.cellHeight}px;height:${m.cellHeight}px;background:${bg}"></div>`;
        }
        if (hiddenRows.length > 0) row = hiddenRows[hiddenRows.length - 1];
      } else if (INTENT_RE.test(text)) {
        const top = row * m.cellHeight;
        html += `<div style="position:absolute;left:0;right:0;top:${top}px;height:${m.cellHeight}px;background:rgba(181,147,90,0.12)"></div>`;
      }
    }
    overlayRef.innerHTML = html;
  }

  function startBlink() {
    stopBlink();
    cursorBlinkOn = true;
    blinkInterval = setInterval(() => {
      cursorBlinkOn = !cursorBlinkOn;
      const m = metrics();
      if (currentFrame && m) {
        repaintCursorRow(currentFrame, m);
      }
    }, 530);
  }

  function stopBlink() {
    if (blinkInterval != null) {
      clearInterval(blinkInterval);
      blinkInterval = undefined;
    }
  }

  function resetBlink() {
    cursorBlinkOn = true;
    startBlink();
  }

  function repaintCursorRow(frame: DecodedFrame, m: CellMetrics) {
    const y = frame.cursorRow * m.cellHeight;
    ctx.clearRect(0, y, canvasRef.width / m.dpr, m.cellHeight);

    const row = frame.rows.find((r) => r.index === frame.cursorRow);
    if (row) {
      const fontFamily = settingsStore.getFontFamily();
      const bgDefault = getComputedStyle(canvasRef).getPropertyValue("--bg-secondary").trim() || "#1e1e1e";
      const fgDefault = getComputedStyle(canvasRef).getPropertyValue("--text-primary").trim() || "#d4d4d4";
      for (let c = 0; c < row.cells.length; c++) {
        const cell = row.cells[c];
        if (cell.char === "") continue;
        const x = c * m.cellWidth;
        const fg = resolveFg(cell, fgDefault);
        const bg = resolveBg(cell, bgDefault);
        if (!cell.defaultBg || cell.inverse) {
          ctx.fillStyle = bg;
          ctx.fillRect(x, y, m.cellWidth, m.cellHeight);
        }
        if (cell.char !== " ") {
          ctx.font = buildFontStyle(cell, m.fontSize, fontFamily);
          ctx.fillStyle = fg;
          if (cell.dim) ctx.globalAlpha = 0.5;
          ctx.fillText(cell.char, x, y + m.baseline);
          if (cell.dim) ctx.globalAlpha = 1.0;
        }
        if (cell.underline) {
          ctx.fillStyle = fg;
          ctx.fillRect(x, y + m.cellHeight - 1, m.cellWidth, 1);
        }
        if (cell.strikeout) {
          ctx.fillStyle = fg;
          ctx.fillRect(x, y + Math.floor(m.cellHeight / 2), m.cellWidth, 1);
        }
      }
    }
    paintSelection(frame, m);
    paintCursor(frame, m);
  }

  function repaintCursorIfNeeded() {
    const m = metrics();
    if (currentFrame && m) repaintCursorRow(currentFrame, m);
  }

  function onFrame(data: ArrayBuffer | number[]) {
    const buffer = data instanceof ArrayBuffer ? data : new Uint8Array(data).buffer;
    const frame = decodeBinaryFrame(buffer);
    if (!frame) return;

    currentFrame = frame;
    const m = metrics();
    if (m) {
      paintFrame(frame, m);
    }
  }

  // --- Link detection on hover ---

  let linkThrottle: ReturnType<typeof setTimeout> | undefined;

  async function checkLinksAtRow(row: number, col: number) {
    if (!invokeRef) return;
    const rowText = await invokeRef("terminal_get_row_text", {
      sessionId: props.sessionId,
      row,
    }) as string;

    const cacheKey = `${row}:${rowText}`;
    let links = linkCache.get(cacheKey);
    if (links === undefined) {
      const fpRe = filePathRegex();
      const fuRe = fileUrlRegex();
      const matches: { text: string; candidate: string; index: number }[] = [];
      let match: RegExpExecArray | null;
      fpRe.lastIndex = 0;
      while ((match = fpRe.exec(rowText)) !== null) {
        const idx = rowText.indexOf(match[1], match.index);
        matches.push({ text: match[1], candidate: match[1], index: idx });
      }
      fuRe.lastIndex = 0;
      while ((match = fuRe.exec(rowText)) !== null) {
        matches.push({ text: match[0], candidate: match[1], index: match.index });
      }

      if (matches.length === 0) {
        linkCache.set(cacheKey, null);
        if (linkCache.size > 200) linkCache.clear();
        links = null;
      } else {
        const termData = terminalsStore.get(props.sessionId);
        const cwd = termData?.cwd || "";
        const resolved = await Promise.all(
          matches.map(async (m) => {
            try {
              const r = await invokeRef!("resolve_terminal_path", { cwd, candidate: m.candidate }) as { absolute_path: string; is_directory: boolean } | null;
              if (!r) return null;
              let line: number | undefined;
              let col: number | undefined;
              const lc = m.candidate.match(/:(\d+)(?::(\d+))?$/);
              if (lc) {
                line = parseInt(lc[1], 10);
                if (lc[2]) col = parseInt(lc[2], 10);
              }
              return { text: m.text, path: r.absolute_path, line, col, index: m.index };
            } catch {
              return null;
            }
          }),
        );
        const validLinks = resolved.filter(Boolean) as { text: string; path: string; line?: number; col?: number; index: number }[];
        links = validLinks.length > 0 ? validLinks : null;
        if (linkCache.size > 200) linkCache.clear();
        linkCache.set(cacheKey, links);
      }
    }

    hoveredLink = null;
    if (links) {
      for (const link of links) {
        const start = link.index ?? 0;
        const end = start + link.text.length;
        if (col >= start && col < end) {
          hoveredLink = { row, colStart: start, colEnd: end, path: link.path, line: link.line, col: link.col };
          break;
        }
      }
    }
    canvasRef.style.cursor = hoveredLink ? "pointer" : "";
  }

  onMount(async () => {
    ctx = canvasRef.getContext("2d")!;
    remeasure();

    resizeObserver = new ResizeObserver(() => remeasure());
    resizeObserver.observe(containerRef);

    canvasRef.addEventListener("focus", () => { setFocused(true); startBlink(); });
    canvasRef.addEventListener("blur", () => { setFocused(false); stopBlink(); repaintCursorIfNeeded(); });

    // --- Keyboard ---
    let composing = false;
    canvasRef.addEventListener("compositionstart", () => { composing = true; });
    canvasRef.addEventListener("compositionend", (e) => {
      composing = false;
      if (e.data) writePty(e.data);
    });

    canvasRef.addEventListener("keydown", (e: KeyboardEvent) => {
      if (composing) return;
      resetBlink();

      // Ctrl+C with selection → copy instead of interrupt
      if ((e.ctrlKey || e.metaKey) && e.key === "c" && selectionStart && selectionEnd) {
        e.preventDefault();
        e.stopPropagation();
        copySelection();
        return;
      }

      // Any keypress clears selection
      if (selectionStart) {
        selectionStart = null;
        selectionEnd = null;
        invokeRef?.("terminal_select_clear", { sessionId: props.sessionId }).catch(() => {});
        const m = metrics();
        if (currentFrame && m) paintFrame(currentFrame, m);
      }

      const seq = keyToSequence(e);
      if (seq !== null) {
        e.preventDefault();
        e.stopPropagation();
        writePty(seq);
      }
    });

    canvasRef.addEventListener("paste", (e: ClipboardEvent) => {
      const text = e.clipboardData?.getData("text");
      if (text) writePty(text);
      e.preventDefault();
    });

    // --- Mouse selection ---
    let clickCount = 0;
    let lastClickTime = 0;

    canvasRef.addEventListener("mousedown", (e: MouseEvent) => {
      if (e.button !== 0) return;
      const pos = canvasToGrid(e);
      const now = Date.now();

      if (now - lastClickTime < 400) {
        clickCount++;
      } else {
        clickCount = 1;
      }
      lastClickTime = now;

      if (clickCount === 2) {
        // Word select
        invokeRef?.("terminal_select_start", { sessionId: props.sessionId, col: pos.col, row: pos.row, word: true }).catch(() => {});
        selectionStart = pos;
        selectionEnd = pos;
      } else if (clickCount >= 3) {
        // Line select
        invokeRef?.("terminal_select_start", { sessionId: props.sessionId, col: 0, row: pos.row }).catch(() => {});
        const m = metrics();
        const maxCol = m ? Math.floor((canvasRef.getBoundingClientRect().width) / m.cellWidth) - 1 : 79;
        invokeRef?.("terminal_select_update", { sessionId: props.sessionId, col: maxCol, row: pos.row }).catch(() => {});
        selectionStart = { col: 0, row: pos.row };
        selectionEnd = { col: maxCol, row: pos.row };
        clickCount = 3;
      } else {
        // Start fresh selection
        selectionStart = pos;
        selectionEnd = null;
        invokeRef?.("terminal_select_start", { sessionId: props.sessionId, col: pos.col, row: pos.row }).catch(() => {});
      }
      selecting = true;
      const m = metrics();
      if (currentFrame && m) paintFrame(currentFrame, m);
    });

    const onMouseMove = (e: MouseEvent) => {
      if (selecting && selectionStart) {
        const pos = canvasToGrid(e);
        selectionEnd = pos;
        invokeRef?.("terminal_select_update", { sessionId: props.sessionId, col: pos.col, row: pos.row }).catch(() => {});
        const m = metrics();
        if (currentFrame && m) paintFrame(currentFrame, m);
      }

      // Link detection (throttled)
      if (!selecting) {
        clearTimeout(linkThrottle);
        linkThrottle = setTimeout(() => {
          const pos = canvasToGrid(e);
          checkLinksAtRow(pos.row, pos.col);
        }, 100);
      }
    };

    const onMouseUp = () => {
      selecting = false;
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);

    // Link click
    canvasRef.addEventListener("click", (e: MouseEvent) => {
      if (hoveredLink && (e.metaKey || e.ctrlKey)) {
        props.onOpenFilePath?.(hoveredLink.path, hoveredLink.line, hoveredLink.col);
      }
    });

    // --- Scroll ---
    canvasRef.addEventListener("wheel", (e: WheelEvent) => {
      e.preventDefault();
      const delta = Math.sign(e.deltaY) * -3;
      invokeRef?.("terminal_scroll", { sessionId: props.sessionId, delta }).catch(() => {});
    }, { passive: false });

    // Subscribe to grid channel
    try {
      const { invoke, Channel } = await import("@tauri-apps/api/core");
      invokeRef = invoke;
      const channel = new Channel<ArrayBuffer | number[]>();
      channel.onmessage = onFrame;
      await invoke("subscribe_terminal_grid", {
        sessionId: props.sessionId,
        channel,
      });
      unsubscribe = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        invoke("unsubscribe_terminal_grid", {
          sessionId: props.sessionId,
        }).catch(() => {});
      };
    } catch {
      // Not in Tauri context (tests, PWA)
    }
  });

  async function copySelection() {
    if (!invokeRef) return;
    try {
      const text = await invokeRef("terminal_select_text", { sessionId: props.sessionId }) as string | null;
      if (text) {
        await navigator.clipboard.writeText(text);
      }
    } catch {
      // clipboard not available
    }
  }

  onCleanup(() => {
    stopBlink();
    resizeObserver?.disconnect();
    unsubscribe?.();
    clearTimeout(linkThrottle);
    linkCache.clear();
  });

  return (
    <div
      ref={containerRef!}
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        overflow: "hidden",
      }}
    >
      <canvas
        ref={canvasRef!}
        style={{
          width: "100%",
          height: "100%",
          display: "block",
        }}
        tabIndex={-1}
      />
      {/* Suggest/intent overlay */}
      <div
        ref={overlayRef!}
        style={{
          position: "absolute",
          top: "0",
          left: "0",
          right: "0",
          bottom: "0",
          "pointer-events": "none",
          "z-index": "10",
          overflow: "hidden",
        }}
      />
      {/* Scrollbar */}
      <div
        ref={scrollbarRef!}
        style={{
          position: "absolute",
          top: "0",
          right: "0",
          width: "8px",
          height: "100%",
          display: "none",
          "z-index": "20",
        }}
      >
        <div
          ref={scrollThumbRef!}
          style={{
            width: "6px",
            "margin-left": "1px",
            "border-radius": "3px",
            background: "rgba(255,255,255,0.25)",
            "min-height": "20px",
            position: "absolute",
            top: "0",
          }}
        />
      </div>
    </div>
  );
};

export default CanvasTerminal;
