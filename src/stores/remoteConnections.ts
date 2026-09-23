import { batch, createSignal } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import { invoke } from "../invoke";
import { setRemoteBaseUrlLookup } from "../transportRuntime";
import { startRemoteEventBridge } from "../utils/remoteEventBridge";
import { appLogger } from "./appLogger";
import type { SshConnectionParams } from "./tunnels";
import { tunnelsStore } from "./tunnels";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface RemoteConnection {
	id: string;
	name: string;
	transport: RemoteTransport;
	/** Optional — no longer required to authenticate anything by itself. */
	auth_username?: string | null;
	enabled: boolean;
}

export type RemoteTransport =
	| {
			type: "Ssh";
			/** Host/port/user/identity/keepalive config — shared shape with `TunnelProfile.ssh`. */
			ssh: SshConnectionParams;
			remote_daemon_port: number;
			/** Remote daemon provisioning (Phase 5): offer to probe/install/start
			 * `tuic-remote` on the remote host if the tunnel fails to connect. */
			start_if_not_running: boolean;
			/** Only meaningful when this session's Connect actually started the
			 * daemon — if false, Disconnect stops it again. */
			leave_running_on_disconnect: boolean;
			/** `--instance <id>` passed when WE launch/configure the daemon
			 * ourselves — an argument we choose, never state read back from the
			 * remote. Distinct from `Local`'s `instance_id`. */
			instance_id: string | null;
	  }
	| {
			type: "Direct";
			url: string;
			/** SHA-256 fingerprint (lowercase hex) of a pinned, self-signed/
			 * untrusted certificate, set once the user explicitly confirms it
			 * (see `probeDirectTls`/`ProbeResult`). `null` for `http://`, a
			 * CA-trusted `https://`, or a self-signed target not yet confirmed. */
			tls_fingerprint: string | null;
	  }
	| {
			type: "Local";
			/** Exactly one of `port`/`instance_id` is set. */
			port: number | null;
			instance_id: string | null;
	  };

/**
 * Test Connection input (story: SSH Tunnels + Remote Servers consolidation,
 * Phase 2) — enough data to test a connection that may not be saved yet.
 * `password` is plaintext, in-memory only for this one call; it is never
 * persisted (not to `connections.json`, not to the keyring).
 */
export interface TestConnectionRequest {
	transport: RemoteTransport;
	auth_username?: string | null;
	password?: string | null;
}

/**
 * Result of `testConnection`. Mirrors the Rust `ConnectionTestResult` enum
 * (`src-tauri/src/connection_test.rs`) — a `#[serde(tag = "type")]` shape, so
 * `result.type` narrows the rest of the fields.
 */
export type ConnectionTestResult =
	| { type: "Reachable" }
	| { type: "AuthFailed" }
	| { type: "NotConfigured" }
	| { type: "InstanceNotFound" }
	| { type: "Unreachable"; reason: string };

/**
 * Result of `probeDirectTls`. Mirrors the Rust `ProbeResult` enum
 * (`src-tauri/src/direct_proxy.rs`) — a `#[serde(tag = "type")]` shape.
 * Story: SSH Tunnels + Remote Servers consolidation, Phase 4.
 */
export type ProbeResult =
	| { type: "NoTlsNeeded" }
	| { type: "Trusted" }
	| { type: "NeedsConfirmation"; fingerprint: string }
	| { type: "PinnedMatch" }
	| { type: "PinnedMismatch"; presented_fingerprint: string };

/**
 * A Direct connection's certificate needs the user's explicit confirmation
 * before it can be pinned and proxied — set by `connect()`, cleared by
 * `resolveFingerprintConfirmation`. Rendered by a dialog in `RemoteServersTab`.
 */
export interface PendingFingerprintConfirmation {
	connectionId: string;
	connectionName: string;
	url: string;
	fingerprint: string;
}

/**
 * Result of `probeSshDaemon`. Mirrors the Rust `SshDaemonState` enum
 * (`src-tauri/src/ssh_provision.rs`). Story: SSH Tunnels + Remote Servers
 * consolidation, Phase 5.
 */
export type SshDaemonState =
	| { type: "Running" }
	| { type: "NotRunningBinaryPresent" }
	| { type: "NotRunningBinaryMissing" };

/** Mirrors the Rust `VersionCheckResult` enum. */
export type VersionCheckResult =
	| { type: "Match" }
	| { type: "Outdated"; remote_version: string; local_version: string };

/**
 * A remote-daemon-provisioning step (Phase 5) needs the user's explicit
 * confirmation before it acts — never a silent default. One at a time, like
 * `PendingFingerprintConfirmation`, rendered by a dialog in `RemoteServersTab`.
 */
export interface PendingProvisionConfirmation {
	connectionId: string;
	connectionName: string;
	/** e.g. "tuic-remote isn't installed on host — download and install it now?" */
	message: string;
	confirmLabel: string;
}

export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "error";

export interface ConnectionState {
	connection: RemoteConnection;
	status: ConnectionStatus;
	baseUrl?: string;
	protocolVersion?: number;
	error?: string;
	tunnelProfileId?: string;
	/** Whether a Direct-transport TLS/auth proxy (Phase 4) is running for
	 * this connection — so `disconnect()` knows whether to stop one. */
	directProxyStarted?: boolean;
	/** Whether THIS session's Connect started the remote daemon itself (Phase
	 * 5) — never persisted; only meaningful for as long as the app runs.
	 * `disconnect()` only ever stops a daemon this session actually started. */
	sshDaemonStartedBySession?: boolean;
	/** Set after a successful connect if the remote's `/api/version` differs
	 * from this app's own version — informational, never blocks the connection. */
	versionWarning?: string;
}

interface RemoteConnectionsState {
	connections: Record<string, ConnectionState>;
	hydrated: boolean;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/** Guard: prevent hydrate from running twice */
let hydrated = false;

/** Active health poll intervals keyed by connection ID */
const healthIntervals = new Map<string, ReturnType<typeof setInterval>>();

/** Active SSE event bridge cleanup functions keyed by connection ID */
const eventBridges = new Map<string, () => void>();

const HEALTH_POLL_MS = 5_000;
const TUNNEL_CONNECT_TIMEOUT_MS = 30_000;
const TUNNEL_POLL_MS = 500;

/** Pick a random local port in [10000, 60000) */
function randomLocalPort(): number {
	return 10_000 + Math.floor(Math.random() * 50_000);
}

function createRemoteConnectionsStore() {
	const [state, setState] = createStore<RemoteConnectionsState>({
		connections: {},
		hydrated: false,
	});

	// Fingerprint confirmation is a one-at-a-time modal flow (Phase 4): `connect()`
	// awaits `requestFingerprintConfirmation`, which resolves once the dialog that
	// reads `pendingConfirmation` calls `resolveFingerprintConfirmation`.
	const [pendingConfirmation, setPendingConfirmation] = createSignal<PendingFingerprintConfirmation | null>(null);
	let confirmationResolver: ((accepted: boolean) => void) | null = null;

	function requestFingerprintConfirmation(
		connectionId: string,
		connectionName: string,
		url: string,
		fingerprint: string,
	): Promise<boolean> {
		return new Promise((resolve) => {
			confirmationResolver = resolve;
			setPendingConfirmation({ connectionId, connectionName, url, fingerprint });
		});
	}

	// Remote daemon provisioning confirmations (Phase 5) — same one-at-a-time
	// pattern as the fingerprint confirmation above, generalized to an
	// arbitrary message/label since provisioning has several distinct
	// confirmable steps (install missing binary, set an unconfigured
	// password, update an outdated binary) rather than one fixed question.
	const [pendingProvisionConfirmation, setPendingProvisionConfirmation] =
		createSignal<PendingProvisionConfirmation | null>(null);
	let provisionConfirmationResolver: ((accepted: boolean) => void) | null = null;

	function requestProvisionConfirmation(
		connectionId: string,
		connectionName: string,
		message: string,
		confirmLabel: string,
	): Promise<boolean> {
		return new Promise((resolve) => {
			provisionConfirmationResolver = resolve;
			setPendingProvisionConfirmation({ connectionId, connectionName, message, confirmLabel });
		});
	}

	// ---------------------------------------------------------------------------
	// Internal helpers
	// ---------------------------------------------------------------------------

	async function pollHealth(id: string): Promise<void> {
		const connState = state.connections[id];
		if (!connState?.baseUrl) return;
		const baseUrl = connState.baseUrl;
		try {
			const resp = await fetch(`${baseUrl}/health`);
			if (resp.ok) {
				const data = (await resp.json()) as { protocol_version?: number };
				setState("connections", id, {
					status: "connected",
					protocolVersion: data.protocol_version,
				});
			} else {
				setState("connections", id, {
					status: "error",
					error: `Health check failed: ${resp.status}`,
				});
			}
		} catch (e) {
			setState("connections", id, {
				status: "error",
				error: `Unreachable: ${e}`,
			});
		}
	}

	function startHealthPolling(id: string): void {
		stopHealthPolling(id);
		const interval = setInterval(() => void pollHealth(id), HEALTH_POLL_MS);
		healthIntervals.set(id, interval);
	}

	function stopHealthPolling(id: string): void {
		const existing = healthIntervals.get(id);
		if (existing !== undefined) {
			clearInterval(existing);
			healthIntervals.delete(id);
		}
	}

	/** Wait for an SSH tunnel to reach connected state (or error/stopped). */
	async function waitForTunnel(profileId: string): Promise<boolean> {
		const deadline = Date.now() + TUNNEL_CONNECT_TIMEOUT_MS;
		while (Date.now() < deadline) {
			const status = tunnelsStore.getTunnelStatus(profileId);
			if (status?.type === "connected") return true;
			if (status?.type === "stopped" || status?.type === "error") {
				appLogger.warn("store", `Tunnel ${profileId} failed: ${status.type}`);
				return false;
			}
			await new Promise<void>((resolve) => setTimeout(resolve, TUNNEL_POLL_MS));
		}
		appLogger.warn("store", `Tunnel ${profileId} connect timeout`);
		return false;
	}

	/** This app's own version — `getVersion()` (Tauri OS API) on desktop; the
	 * same `/api/version` a remote daemon serves, but against the CURRENT
	 * origin, in browser mode (there is no Tauri command for it — desktop
	 * doesn't need one, browser mode already has same-origin `fetch`). */
	async function getLocalAppVersion(): Promise<string | null> {
		try {
			const { isTauri } = await import("../transport");
			if (isTauri()) {
				const { getVersion } = await import("@tauri-apps/api/app");
				return await getVersion();
			}
			const resp = await fetch("/api/version");
			if (!resp.ok) return null;
			const data = (await resp.json()) as { version?: string };
			return data.version ?? null;
		} catch {
			return null;
		}
	}

	/** After a successful connect, best-effort check whether the remote's
	 * version differs from this app's own — informational only, never blocks
	 * or retries the connection. Sets `versionWarning` when they differ. */
	async function checkRemoteVersionAfterConnect(id: string, baseUrl: string): Promise<void> {
		try {
			const [localVersion, remoteResp] = await Promise.all([getLocalAppVersion(), fetch(`${baseUrl}/api/version`)]);
			if (!localVersion || !remoteResp.ok) return;
			const remoteData = (await remoteResp.json()) as { version?: string };
			if (!remoteData.version) return;
			const result = await invoke<VersionCheckResult>("check_remote_version", {
				localVersion,
				remoteVersion: remoteData.version,
			});
			if (result.type === "Outdated") {
				setState("connections", id, {
					versionWarning: `Remote is running version ${result.remote_version}, this app is on ${result.local_version}.`,
				});
			}
		} catch (err) {
			// Best-effort — a version-check failure must never affect the
			// connection's actual status.
			appLogger.warn("store", `Version check failed for connection ${id}`, err);
		}
	}

	/**
	 * Remote daemon provisioning (Phase 5): called only after the normal
	 * tunnel-connect attempt has already failed and `start_if_not_running` is
	 * set. Probes whether `tuic-remote` is running/installed on the remote
	 * host, and — ALWAYS with an explicit confirmation dialog first, never
	 * silently — installs it if missing and starts it. Returns `true` if the
	 * caller should retry the tunnel connect, `false` if provisioning didn't
	 * get the daemon running (the caller should report the original error).
	 */
	async function ensureSshDaemonRunning(
		id: string,
		connectionName: string,
		ssh: SshConnectionParams,
		remoteDaemonPort: number,
		instanceId: string | null,
	): Promise<boolean> {
		const state_ = await invoke<SshDaemonState>("probe_ssh_daemon", { ssh, port: remoteDaemonPort });

		if (state_.type === "NotRunningBinaryMissing") {
			const accepted = await requestProvisionConfirmation(
				id,
				connectionName,
				`tuic-remote isn't installed on ${ssh.host}. Download and install it now?`,
				"Install",
			);
			if (!accepted) return false;
			await invoke("install_ssh_daemon", { ssh });
		} else if (state_.type === "Running") {
			// Already running but the tunnel still failed to connect — some
			// other problem (host key, auth, a stale forward). Don't mask it
			// with a provisioning flow that has nothing to offer here.
			return false;
		}

		const startAccepted = await requestProvisionConfirmation(
			id,
			connectionName,
			`${connectionName}'s remote daemon isn't running on ${ssh.host}. Start it now?`,
			"Start",
		);
		if (!startAccepted) return false;

		await invoke("start_ssh_remote_daemon", { ssh, instanceId, port: remoteDaemonPort });
		setState("connections", id, { sshDaemonStartedBySession: true });
		return true;
	}

	/**
	 * After a (re)connected SSH tunnel's first health check, offer to set the
	 * remote daemon's password if it turns out to have never been configured
	 * (distinct from a wrong password — see `mcp_http/auth.rs`'s
	 * `AuthResult::NotConfigured` vs `Invalid`). The credentials come from the
	 * connection's OWN already-saved `auth_username` + keyring entry, sourced
	 * entirely server-side (`configure_ssh_daemon_password`) — `connect()`'s
	 * caller here never has the plaintext password in hand at all (only the
	 * Settings editor's in-progress form does, a separate flow). Best-effort:
	 * any failure here just leaves the connection in its current
	 * (already-reported) state.
	 */
	async function offerToConfigureIfUnconfigured(
		id: string,
		connectionName: string,
		hasAuthUsername: boolean,
		baseUrl: string,
	): Promise<void> {
		if (!hasAuthUsername) return;
		try {
			const resp = await fetch(`${baseUrl}/health`);
			if (resp.status !== 401) return;
			const bodyText = await resp.text().catch(() => "");
			// "Scan the QR code or authenticate with Basic Auth" (NotConfigured
			// /MissingHeader) is textually distinct from "Invalid credentials"
			// (Invalid) — see mcp_http/auth.rs. A wrong password must NEVER
			// trigger this offer, only a daemon that has no credentials at all.
			if (!bodyText.includes("Scan the QR code")) return;
		} catch {
			return;
		}

		const accepted = await requestProvisionConfirmation(
			id,
			connectionName,
			`${connectionName}'s remote daemon has no password configured yet. Set it to this connection's saved credentials?`,
			"Set password",
		);
		if (!accepted) return;
		try {
			await invoke("configure_ssh_daemon_password", { connectionId: id });
		} catch (err) {
			appLogger.error("store", `Failed to set remote daemon password for connection ${id}`, err);
		}
	}

	// ---------------------------------------------------------------------------
	// Actions
	// ---------------------------------------------------------------------------

	const actions = {
		/** Load connections from backend, set hydrated */
		async hydrate(): Promise<void> {
			if (hydrated) return;
			try {
				const connections = await invoke<RemoteConnection[]>("list_remote_connections");
				const connectionsMap: Record<string, ConnectionState> = {};
				for (const conn of connections ?? []) {
					connectionsMap[conn.id] = { connection: conn, status: "disconnected" };
				}
				// `reconcile`, not a plain-object `setState`, which SolidJS store
				// setters MERGE onto the existing key rather than replace — a
				// connection the backend no longer reports would otherwise linger
				// in state forever (fixed, story: SSH Tunnels + Remote Servers
				// consolidation).
				batch(() => {
					setState("connections", reconcile(connectionsMap));
					setState("hydrated", true);
				});
				hydrated = true;
			} catch (err) {
				appLogger.error("store", "Failed to hydrate remote connections", err);
			}
		},

		/**
		 * Connect to a remote connection.
		 * - SSH: creates a tunnel profile, starts it, sets baseUrl to the local port.
		 * - Direct: sets baseUrl to the configured URL directly.
		 * In both cases, health polling begins once the baseUrl is set.
		 */
		async connect(id: string): Promise<void> {
			const connState = state.connections[id];
			if (!connState) {
				appLogger.warn("store", `connect: unknown connection ${id}`);
				return;
			}
			if (connState.status === "connecting" || connState.status === "connected") return;

			setState("connections", id, { status: "connecting", error: undefined });
			appLogger.info("store", `Connecting remote connection ${id} (${connState.connection.name})`);

			const { transport } = connState.connection;

			try {
				if (transport.type === "Ssh") {
					// Captured as its own const: TS narrowing on `transport` doesn't
					// survive into the nested function expression below (a function
					// boundary resets narrowing on a captured outer variable) — this
					// alias keeps `.ssh`/`.remote_daemon_port`/etc. typed correctly
					// wherever it's used in this block.
					const sshTransport = transport;
					const profileName = `__remote_${id}`;

					/** Create/reuse the tunnel profile, start it, and wait for it to
					 * connect. Called twice when provisioning kicks in: once for the
					 * initial attempt, once more after the daemon has (hopefully)
					 * been started on the remote host. */
					async function attemptTunnelConnect(): Promise<{ localPort: number; profileId: string } | null> {
						const localPort = randomLocalPort();
						await tunnelsStore.createProfile({
							name: profileName,
							ssh: {
								...sshTransport.ssh,
								// A remote-connection-managed tunnel always accepts a
								// newly-seen host key rather than prompting — there's no
								// interactive terminal attached to answer ssh's prompt.
								strict_host_key_checking: "AcceptNew",
							},
							forwards: [
								{
									type: "Local",
									bind_port: localPort,
									remote_host: "127.0.0.1",
									remote_port: sshTransport.remote_daemon_port,
								},
							],
							auto_connect: false,
						});

						await tunnelsStore.refreshProfiles();
						const profile = tunnelsStore.getProfiles().find((p) => p.name === profileName);
						if (!profile) {
							throw new Error(`Could not find tunnel profile "${profileName}" after creation`);
						}

						await tunnelsStore.startTunnel(profile.id);
						const connected = await waitForTunnel(profile.id);
						return connected ? { localPort, profileId: profile.id } : null;
					}

					let result = await attemptTunnelConnect();

					if (!result && sshTransport.start_if_not_running) {
						// Remote daemon provisioning (Phase 5): the tunnel failing to
						// connect at all (not an auth/host-key failure once actually
						// connected) is the "not running" signal — always confirmed
						// with the user before acting, never silently.
						const provisioned = await ensureSshDaemonRunning(
							id,
							connState.connection.name,
							sshTransport.ssh,
							sshTransport.remote_daemon_port,
							sshTransport.instance_id,
						);
						if (provisioned) {
							result = await attemptTunnelConnect();
						}
					}

					if (!result) {
						setState("connections", id, {
							status: "error",
							error: "SSH tunnel failed to connect",
						});
						return;
					}

					const baseUrl = `http://127.0.0.1:${result.localPort}`;
					setState("connections", id, {
						baseUrl,
						tunnelProfileId: result.profileId,
					});

					// Initial health check sets status to "connected" or "error"
					await pollHealth(id);
					startHealthPolling(id);
					eventBridges.get(id)?.();
					eventBridges.set(id, startRemoteEventBridge(id, baseUrl));

					await offerToConfigureIfUnconfigured(
						id,
						connState.connection.name,
						!!connState.connection.auth_username,
						baseUrl,
					);
					await checkRemoteVersionAfterConnect(id, baseUrl);
				} else if (transport.type === "Direct") {
					// Direct transport (story: SSH Tunnels + Remote Servers
					// consolidation, Phase 4). Probe first: a plain `http://` or an
					// already-CA-trusted `https://` with no credentials configured
					// talks straight to the URL exactly as before Phase 4. Anything
					// else (auth configured, or a self-signed cert) needs the local
					// TLS/auth proxy — see `direct_proxy.rs`'s module doc comment for
					// why the password never crosses into this frontend code.
					const probe = await invoke<ProbeResult>("probe_direct_tls_connection", {
						url: transport.url,
						tlsFingerprint: transport.tls_fingerprint,
					});

					let tlsFingerprint: string | null = null;
					let useNativeRoots = false;
					if (probe.type === "Trusted") {
						useNativeRoots = true;
					} else if (probe.type === "PinnedMatch") {
						tlsFingerprint = transport.tls_fingerprint;
					} else if (probe.type === "PinnedMismatch") {
						setState("connections", id, {
							status: "error",
							error: `Certificate changed: expected fingerprint ${transport.tls_fingerprint}, server now presents ${probe.presented_fingerprint}. Refusing to connect automatically — verify the server before retrying.`,
						});
						return;
					} else if (probe.type === "NeedsConfirmation") {
						const accepted = await requestFingerprintConfirmation(
							id,
							connState.connection.name,
							transport.url,
							probe.fingerprint,
						);
						if (!accepted) {
							setState("connections", id, { status: "disconnected" });
							return;
						}
						tlsFingerprint = probe.fingerprint;
						const pinned: RemoteConnection = {
							...connState.connection,
							transport: { ...transport, tls_fingerprint: tlsFingerprint },
						};
						await actions.addConnection(pinned);
					}
					// `NoTlsNeeded` needs neither flag — plain TCP, no auth-only proxy
					// unless credentials are configured, which `start_direct_proxy`
					// itself checks server-side.

					const proxyPort = await invoke<number | null>("start_direct_proxy", {
						connectionId: id,
						url: transport.url,
						tlsFingerprint,
						useNativeRoots,
					});
					const baseUrl = proxyPort ? `http://127.0.0.1:${proxyPort}` : transport.url;
					setState("connections", id, { baseUrl, directProxyStarted: proxyPort !== null });
					await pollHealth(id);
					startHealthPolling(id);
					eventBridges.get(id)?.();
					eventBridges.set(id, startRemoteEventBridge(id, baseUrl));
					await checkRemoteVersionAfterConnect(id, baseUrl);
				} else {
					// Local transport: another named/isolated instance on this same
					// machine. Always plain HTTP on loopback — never TLS (loopback is
					// already a browser secure context) — so this reuses the Direct
					// proxy machinery unchanged (Phase 4) purely for its auth-injection
					// capability: `start_direct_proxy` only actually starts a proxy when
					// credentials are configured, otherwise it's a no-op and baseUrl is
					// the resolved URL directly, exactly like an unauthenticated Direct
					// http:// connection today.
					const port = transport.instance_id
						? await invoke<number>("get_local_instance_port", { instanceId: transport.instance_id })
						: transport.port;
					if (!port) {
						throw new Error("Local connection has neither an instance_id nor a port configured");
					}
					const url = `http://127.0.0.1:${port}`;
					const proxyPort = await invoke<number | null>("start_direct_proxy", {
						connectionId: id,
						url,
						tlsFingerprint: null,
						useNativeRoots: false,
					});
					const baseUrl = proxyPort ? `http://127.0.0.1:${proxyPort}` : url;
					setState("connections", id, { baseUrl, directProxyStarted: proxyPort !== null });
					await pollHealth(id);
					startHealthPolling(id);
					eventBridges.get(id)?.();
					eventBridges.set(id, startRemoteEventBridge(id, baseUrl));
					await checkRemoteVersionAfterConnect(id, baseUrl);
				}
			} catch (err) {
				appLogger.error("store", `Failed to connect remote connection ${id}`, err);
				setState("connections", id, {
					status: "error",
					error: String(err),
				});
			}
		},

		/** Disconnect from a remote connection. Stops the SSH tunnel if applicable. */
		async disconnect(id: string): Promise<void> {
			const connState = state.connections[id];
			if (!connState) return;

			stopHealthPolling(id);
			const bridgeCleanup = eventBridges.get(id);
			if (bridgeCleanup) {
				bridgeCleanup();
				eventBridges.delete(id);
			}

			const { tunnelProfileId, directProxyStarted, sshDaemonStartedBySession } = connState;
			if (tunnelProfileId) {
				try {
					await tunnelsStore.stopTunnel(tunnelProfileId);
					// Clean up the auto-created profile
					await tunnelsStore.deleteProfile(tunnelProfileId);
				} catch (err) {
					appLogger.warn("store", `Failed to stop/delete tunnel for connection ${id}`, err);
				}
			}
			if (directProxyStarted) {
				try {
					await invoke("stop_direct_proxy", { connectionId: id });
				} catch (err) {
					appLogger.warn("store", `Failed to stop direct proxy for connection ${id}`, err);
				}
			}
			// Remote daemon provisioning (Phase 5): only ever stop a daemon THIS
			// session actually started, and only when the connection's own
			// "leave running" setting doesn't say to keep it up.
			if (
				sshDaemonStartedBySession &&
				connState.connection.transport.type === "Ssh" &&
				!connState.connection.transport.leave_running_on_disconnect
			) {
				try {
					await invoke("stop_ssh_remote_daemon", {
						ssh: connState.connection.transport.ssh,
						port: connState.connection.transport.remote_daemon_port,
					});
				} catch (err) {
					appLogger.warn("store", `Failed to stop remote daemon for connection ${id}`, err);
				}
			}

			setState("connections", id, {
				status: "disconnected",
				baseUrl: undefined,
				protocolVersion: undefined,
				error: undefined,
				tunnelProfileId: undefined,
				directProxyStarted: undefined,
				sshDaemonStartedBySession: undefined,
				versionWarning: undefined,
			});
			appLogger.info("store", `Disconnected remote connection ${id}`);
		},

		/** Save a new connection to the backend and add it to state */
		async addConnection(conn: RemoteConnection): Promise<void> {
			try {
				await invoke("save_remote_connection", { connection: conn });
				setState("connections", conn.id, { connection: conn, status: "disconnected" });
			} catch (err) {
				appLogger.error("store", "Failed to save remote connection", err);
				throw err;
			}
		},

		/** Disconnect (if connected), delete from backend, and remove from state */
		async removeConnection(id: string): Promise<void> {
			const connState = state.connections[id];
			if (!connState) return;

			if (connState.status === "connecting" || connState.status === "connected") {
				await actions.disconnect(id);
			}

			try {
				await invoke("delete_remote_connection", { id });
				setState(
					produce((s) => {
						delete s.connections[id];
					}),
				);
			} catch (err) {
				appLogger.error("store", `Failed to delete remote connection ${id}`, err);
				throw err;
			}
		},

		/**
		 * Returns the baseUrl for a connected connection, or undefined if not connected.
		 * This is the primary API used by transport routing (Step 17).
		 */
		getBaseUrl(connectionId: string): string | undefined {
			const connState = state.connections[connectionId];
			if (connState?.status === "connected" && connState.baseUrl) {
				return connState.baseUrl;
			}
			return undefined;
		},

		/** Reactive getter for all connections */
		getConnections(): Record<string, ConnectionState> {
			return state.connections;
		},

		/** Reactive getter for a single connection's state */
		getConnectionState(id: string): ConnectionState | undefined {
			return state.connections[id];
		},

		/**
		 * SSH-only "Update" action for an outdated remote daemon (Phase 5):
		 * repeats the same download/replace step `ensureSshDaemonRunning` uses
		 * for a missing binary, then restarts the process via the same
		 * PID-verified stop/start pair Disconnect uses. Never called
		 * automatically — always an explicit user action from the version
		 * warning shown after Connect.
		 */
		async updateSshRemoteBinary(id: string): Promise<void> {
			const connState = state.connections[id];
			if (connState?.connection.transport.type !== "Ssh") return;
			const { ssh, remote_daemon_port, instance_id } = connState.connection.transport;
			await invoke("install_ssh_daemon", { ssh });
			try {
				await invoke("stop_ssh_remote_daemon", { ssh, port: remote_daemon_port });
			} catch (err) {
				// The daemon may not have been running under a PID we can verify
				// (e.g. started outside this app) — installing the new binary
				// still succeeded, so proceed to (re)start it regardless.
				appLogger.warn("store", `Failed to stop remote daemon before update for connection ${id}`, err);
			}
			await invoke("start_ssh_remote_daemon", { ssh, instanceId: instance_id, port: remote_daemon_port });
			setState("connections", id, { versionWarning: undefined });
		},

		/**
		 * Test connectivity for a transport that may not be saved yet (Phase 2 —
		 * no UI wired to this yet, Phase 3 owns that). Never mutates store state:
		 * a pure passthrough to the backend, which itself has no side effects
		 * beyond the outbound probe (no tunnel started, nothing persisted).
		 */
		async testConnection(request: TestConnectionRequest): Promise<ConnectionTestResult> {
			return invoke<ConnectionTestResult>("test_connection", { request });
		},

		/**
		 * Whether a password is currently stored in the keyring for this
		 * connection id — never returns the password itself. Used by the
		 * editor to show "password set" without round-tripping the secret.
		 */
		async connectionPasswordExists(id: string): Promise<boolean> {
			return invoke<boolean>("remote_connection_password_exists", { id });
		},

		/**
		 * Save (or overwrite) this connection's password in the OS keyring.
		 * Never written to `connections.json` — see `RemoteConnection.auth_username`'s
		 * doc comment in `remote_connection.rs`.
		 */
		async saveConnectionPassword(id: string, password: string): Promise<void> {
			await invoke("save_remote_connection_password", { id, password });
		},

		/** Clear a connection's stored password from the keyring. */
		async deleteConnectionPassword(id: string): Promise<void> {
			await invoke("delete_remote_connection_password", { id });
		},

		/** Reactive getter for a pending self-signed-cert confirmation, if any
		 * Direct connection is currently mid-Connect and awaiting one. */
		getPendingFingerprintConfirmation(): PendingFingerprintConfirmation | null {
			return pendingConfirmation();
		},

		/** Resolve the current pending fingerprint confirmation — `true` pins the
		 * fingerprint and lets `connect()` proceed, `false` aborts the connect. */
		resolveFingerprintConfirmation(accepted: boolean): void {
			setPendingConfirmation(null);
			confirmationResolver?.(accepted);
			confirmationResolver = null;
		},

		/** Reactive getter for a pending remote-daemon-provisioning confirmation
		 * (Phase 5), if any SSH connection is mid-Connect and awaiting one. */
		getPendingProvisionConfirmation(): PendingProvisionConfirmation | null {
			return pendingProvisionConfirmation();
		},

		/** Resolve the current pending provisioning confirmation. */
		resolveProvisionConfirmation(accepted: boolean): void {
			setPendingProvisionConfirmation(null);
			provisionConfirmationResolver?.(accepted);
			provisionConfirmationResolver = null;
		},
	};

	return {
		state,
		...actions,
	};
}

export const remoteConnectionsStore = createRemoteConnectionsStore();
setRemoteBaseUrlLookup((connectionId) => remoteConnectionsStore.getBaseUrl(connectionId));
