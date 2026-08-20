import { describe, expect, it } from "vitest";

import { decodeBinaryFrame, resolveCursorShape } from "../canvasTerminalUtils";

/**
 * Regression coverage for the "cursor caught before the last character" bug:
 * DECCKM (application cursor keys) rides in `keyboard_flags` bit6 (frame_flags is
 * full — see serialize_dirty_rows), and the cursor-shape "app has not requested a
 * shape" sentinel rides in `frame_flags` bits 1-2 value 3.
 */
function header(opts: { keyboardFlags?: number; frameFlags?: number }): ArrayBuffer {
	const buf = new ArrayBuffer(26);
	const view = new DataView(buf);
	let offset = 0;
	view.setUint16(offset, 0, true); // row_count
	offset += 2;
	view.setUint16(offset, 0, true); // cursor_row
	offset += 2;
	view.setUint16(offset, 0, true); // cursor_col
	offset += 2;
	view.setUint8(offset, 1); // cursor_visible
	offset += 1;
	view.setUint32(offset, 0, true); // display_offset
	offset += 4;
	view.setUint32(offset, 0, true); // history_size
	offset += 4;
	view.setUint8(offset, 0); // has_selection
	offset += 1;
	view.setUint8(offset, opts.keyboardFlags ?? 0);
	offset += 1;
	view.setUint8(offset, opts.frameFlags ?? 0);
	offset += 1;
	view.setUint16(offset, 24, true); // num_lines
	offset += 2;
	view.setUint16(offset, 80, true); // num_cols
	offset += 2;
	view.setUint32(offset, 0, true); // history_base
	return buf;
}

describe("decodeBinaryFrame — DECCKM (application cursor keys)", () => {
	it("reports appCursor=false by default", () => {
		expect(decodeBinaryFrame(header({}))?.appCursor).toBe(false);
	});

	it("reports appCursor=true when keyboard_flags bit6 is set", () => {
		expect(decodeBinaryFrame(header({ keyboardFlags: 0x40 }))?.appCursor).toBe(true);
	});

	it("keeps the real keyboard bits and the alt-screen bit independent of appCursor", () => {
		// All five kitty-keyboard bits + alt-screen + app-cursor all set.
		const frame = decodeBinaryFrame(header({ keyboardFlags: 0x7f }));
		expect(frame?.keyboardFlags).toBe(0x1f);
		expect(frame?.altScreen).toBe(true);
		expect(frame?.appCursor).toBe(true);
	});
});

describe("decodeBinaryFrame — cursor shape", () => {
	it("decodes shape bits 0-2 as block/underline/beam", () => {
		expect(decodeBinaryFrame(header({ frameFlags: 0x00 }))?.cursorShape).toBe("block");
		expect(decodeBinaryFrame(header({ frameFlags: 0x02 }))?.cursorShape).toBe("underline");
		expect(decodeBinaryFrame(header({ frameFlags: 0x04 }))?.cursorShape).toBe("beam");
	});

	it('decodes shape bits value 3 as "default" (no DECSCUSR seen)', () => {
		expect(decodeBinaryFrame(header({ frameFlags: 0x06 }))?.cursorShape).toBe("default");
	});
});

describe("resolveCursorShape", () => {
	it("falls back to the user's setting when the app never sent DECSCUSR", () => {
		expect(resolveCursorShape("default", "beam")).toBe("beam");
		expect(resolveCursorShape("default", "block")).toBe("block");
		expect(resolveCursorShape("default", "underline")).toBe("underline");
	});

	it("honors an explicit app-requested shape over the setting — including block", () => {
		// This is the bug: an app requesting a block cursor (e.g. zsh vi-mode's
		// vicmd DECSCUSR) must win even when the user's setting is "bar" (beam).
		expect(resolveCursorShape("block", "beam")).toBe("block");
		expect(resolveCursorShape("beam", "block")).toBe("beam");
		expect(resolveCursorShape("underline", "beam")).toBe("underline");
	});
});
