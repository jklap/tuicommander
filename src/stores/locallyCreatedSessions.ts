/** PTY sessions created by THIS client (desktop or browser). Since B.4 removed
 *  the `beforeunload` auto-close, this set's only remaining purpose is to
 *  drop the create-echo for a session this client is itself mid-creating —
 *  never "sessions we might kill" (nothing kills a session on unload anymore,
 *  on either transport).
 *
 *  A leaf module on purpose: `usePty` (writer) and `useAppInit` (reader) both
 *  import it, and `usePty -> useAppInit` closed a runtime import cycle through
 *  `pluginRegistry -> startWork -> usePty`. */
export const locallyCreatedSessions = new Set<string>();
