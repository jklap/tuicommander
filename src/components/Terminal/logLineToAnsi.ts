import type { LogLine, LogSpan, LogColor } from "../../mobile/utils/logLine";

function colorToAnsi(color: LogColor, base: number): number[] {
  if (color.rgb) return [base + 8, 2, ...color.rgb];
  if (color.idx !== undefined) {
    const idx = color.idx;
    if (idx < 8) return [base + idx];
    if (idx < 16) return [base + 60 + idx - 8];
    return [base + 8, 5, idx];
  }
  return [];
}

export function logSpanToAnsi(span: LogSpan): string {
  const codes: number[] = [];
  if (span.bold) codes.push(1);
  if (span.italic) codes.push(3);
  if (span.underline) codes.push(4);
  if (span.fg) codes.push(...colorToAnsi(span.fg, 30));
  if (span.bg) codes.push(...colorToAnsi(span.bg, 40));
  if (codes.length === 0) return span.text;
  return `\x1b[${codes.join(";")}m${span.text}\x1b[0m`;
}

export function logLineToAnsi(line: LogLine): string {
  return line.spans.map(logSpanToAnsi).join("");
}

export function logLinesToAnsi(lines: LogLine[]): string {
  return lines.map(logLineToAnsi).join("\r\n");
}
