/** PTY sessions created by THIS client (desktop or browser). Since B.4 removed
 *  the `beforeunload` auto-close, this set's only remaining purpose is to
 *  drop the create-echo for a session this client is itself mid-creating —
 *  never "sessions we might kill" (nothing in useAppInit kills a session on
 *  unload anymore, on either transport).
 *
 *  Its own module so usePty can record ids without importing useAppInit
 *  (usePty → useAppInit → pluginRegistry → startWork → usePty is a runtime
 *  import cycle; scripts/check-frontend-cycles.mjs). */
export const locallyCreatedSessions = new Set<string>();
