use tuic_terminal::vt_log::VtLogBuffer;

// Catches HistoryOnly truncating visible text on shrink or losing the natural
// WRAPLINE chain on grow, before any consumer performs secret redaction.
#[test]
fn resize_preserves_complete_visible_logical_text_1407() {
    let text = "echo GITHUB_TOKEN=ghp_0123456789abcdefghijklmnopqrstuvwxyzAB";
    for (from, to) in [(220, 80), (80, 30), (30, 220), (40, 12)] {
        let mut log = VtLogBuffer::new(24, from, 100);
        let _ = log.process(format!("{text}\r\n").as_bytes());
        log.resize(24, to);
        let total = log.grid_total_lines();
        let copy = log
            .grid_get_selection_text(0, 0, total - 1, log.grid_columns() - 1, None)
            .expect("retained terminal text");
        assert!(
            copy.contains(text),
            "resize {from}->{to} lost or split logical content: {copy:?}"
        );
    }
}
