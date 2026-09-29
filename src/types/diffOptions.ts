/** Mirrors Rust's `diff_options::DiffOptions` (src-tauri/src/diff_options.rs). Field
 * names match its `#[serde(rename_all = "camelCase")]` wire shape exactly, and are
 * also the HTTP query-parameter names `mcp_http/types.rs`'s `PathQuery`/`FileQuery`
 * expect. All four are independent and combinable; every field omitted/false is the
 * ordinary byte-exact diff every existing caller already gets. */
export interface DiffOptions {
	ignoreLeadingWs?: boolean;
	ignoreTrailingWs?: boolean;
	ignoreWsAmount?: boolean;
	ignoreCase?: boolean;
}
