import { describe, it, expect } from "vitest";
import {
  logSpanToAnsi,
  logLineToAnsi,
  logLinesToAnsi,
} from "../../../components/Terminal/logLineToAnsi";
import type { LogSpan, LogLine } from "../../../mobile/utils/logLine";

describe("logSpanToAnsi", () => {
  it("returns plain text when no attributes", () => {
    const span: LogSpan = { text: "hello" };
    expect(logSpanToAnsi(span)).toBe("hello");
  });

  it("handles fg color index 0-7", () => {
    const span: LogSpan = { text: "red", fg: { idx: 1 } };
    expect(logSpanToAnsi(span)).toBe("\x1b[31mred\x1b[0m");
  });

  it("handles fg color index 8-15 (bright)", () => {
    const span: LogSpan = { text: "bright-red", fg: { idx: 9 } };
    // idx 9 → bright red: 90 + (9-8) = 91
    expect(logSpanToAnsi(span)).toBe("\x1b[91mbright-red\x1b[0m");
  });

  it("handles fg color index 16-255 (256-color)", () => {
    const span: LogSpan = { text: "256color", fg: { idx: 200 } };
    expect(logSpanToAnsi(span)).toBe("\x1b[38;5;200m256color\x1b[0m");
  });

  it("handles fg RGB color", () => {
    const span: LogSpan = { text: "rgb", fg: { rgb: [255, 128, 0] } };
    expect(logSpanToAnsi(span)).toBe("\x1b[38;2;255;128;0mrgb\x1b[0m");
  });

  it("handles bg color index 0-7", () => {
    const span: LogSpan = { text: "bg-blue", bg: { idx: 4 } };
    expect(logSpanToAnsi(span)).toBe("\x1b[44mbg-blue\x1b[0m");
  });

  it("handles bg color index 8-15 (bright)", () => {
    const span: LogSpan = { text: "bg-bright-blue", bg: { idx: 12 } };
    // idx 12 → bright blue bg: 100 + (12-8) = 104
    expect(logSpanToAnsi(span)).toBe("\x1b[104mbg-bright-blue\x1b[0m");
  });

  it("handles bg color index 16-255 (256-color)", () => {
    const span: LogSpan = { text: "bg256", bg: { idx: 100 } };
    expect(logSpanToAnsi(span)).toBe("\x1b[48;5;100mbg256\x1b[0m");
  });

  it("handles bg RGB color", () => {
    const span: LogSpan = { text: "bgrgb", bg: { rgb: [0, 128, 255] } };
    expect(logSpanToAnsi(span)).toBe("\x1b[48;2;0;128;255mbgrgb\x1b[0m");
  });

  it("handles bold", () => {
    const span: LogSpan = { text: "bold", bold: true };
    expect(logSpanToAnsi(span)).toBe("\x1b[1mbold\x1b[0m");
  });

  it("handles italic", () => {
    const span: LogSpan = { text: "italic", italic: true };
    expect(logSpanToAnsi(span)).toBe("\x1b[3mitalic\x1b[0m");
  });

  it("handles underline", () => {
    const span: LogSpan = { text: "underline", underline: true };
    expect(logSpanToAnsi(span)).toBe("\x1b[4munderline\x1b[0m");
  });

  it("handles mixed attributes: bold + fg + bg", () => {
    const span: LogSpan = { text: "mixed", bold: true, fg: { idx: 2 }, bg: { idx: 5 } };
    expect(logSpanToAnsi(span)).toBe("\x1b[1;32;45mmixed\x1b[0m");
  });

  it("handles mixed attributes: italic + underline + RGB fg", () => {
    const span: LogSpan = { text: "combo", italic: true, underline: true, fg: { rgb: [10, 20, 30] } };
    expect(logSpanToAnsi(span)).toBe("\x1b[3;4;38;2;10;20;30mcombo\x1b[0m");
  });
});

describe("logLineToAnsi", () => {
  it("returns empty string for empty spans", () => {
    const line: LogLine = { spans: [] };
    expect(logLineToAnsi(line)).toBe("");
  });

  it("concatenates single plain span", () => {
    const line: LogLine = { spans: [{ text: "hello" }] };
    expect(logLineToAnsi(line)).toBe("hello");
  });

  it("concatenates multiple spans", () => {
    const line: LogLine = {
      spans: [
        { text: "a" },
        { text: "b", bold: true },
        { text: "c", fg: { idx: 1 } },
      ],
    };
    expect(logLineToAnsi(line)).toBe("a\x1b[1mb\x1b[0m\x1b[31mc\x1b[0m");
  });
});

describe("logLinesToAnsi", () => {
  it("returns empty string for empty array", () => {
    expect(logLinesToAnsi([])).toBe("");
  });

  it("joins lines with \\r\\n", () => {
    const lines: LogLine[] = [
      { spans: [{ text: "line1" }] },
      { spans: [{ text: "line2" }] },
    ];
    expect(logLinesToAnsi(lines)).toBe("line1\r\nline2");
  });

  it("handles empty line in the middle", () => {
    const lines: LogLine[] = [
      { spans: [{ text: "a" }] },
      { spans: [] },
      { spans: [{ text: "b" }] },
    ];
    expect(logLinesToAnsi(lines)).toBe("a\r\n\r\nb");
  });
});
