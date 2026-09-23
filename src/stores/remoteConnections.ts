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
					const localPort = randomLocalPort();
					const profileName = `__remote_${id}`;

					// Create (or re-use) a tunnel profile for this connection
					await tunnelsStore.createProfile({
						name: profileName,
						ssh: {
							...transport.ssh,
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
								remote_port: transport.remote_daemon_port,
							},
						],
						auto_connect: false,
					});

					// Find the profile ID we just created (by name)
					await tunnelsStore.refreshProfiles();
					const profiles = tunnelsStore.getProfiles();
					const profile = profiles.find((p) => p.name === profileName);
					if (!profile) {
						throw new Error(`Could not find tunnel profile "${profileName}" after creation`);
					}

					// Start the tunnel and wait for it to connect
					await tunnelsStore.startTunnel(profile.id);
					const connected = await waitForTunnel(profile.id);
					if (!connected) {
						setState("connections", id, {
							status: "error",
							error: "SSH tunnel failed to connect",
						});
						return;
					}

					const baseUrl = `http://127.0.0.1:${localPort}`;
					setState("connections", id, {
						baseUrl,
						tunnelProfileId: profile.id,
					});

					// Initial health check sets status to "connected" or "error"
					await pollHealth(id);
					startHealthPolling(id);
					eventBridges.get(id)?.();
					eventBridges.set(id, startRemoteEventBridge(id, baseUrl));
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
				} else {
					// Local transport — instance-id/port resolution and the actual
					// connect flow land in a later phase of the SSH Tunnels + Remote
					// Servers consolidation plan (Phase 1 only introduces the shape).
					throw new Error("Local connections are not yet supported");
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

			const { tunnelProfileId, directProxyStarted } = connState;
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

			setState("connections", id, {
				status: "disconnected",
				baseUrl: undefined,
				protocolVersion: undefined,
				error: undefined,
				tunnelProfileId: undefined,
				directProxyStarted: undefined,
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
	};

	return {
		state,
		...actions,
	};
}

export const remoteConnectionsStore = createRemoteConnectionsStore();
setRemoteBaseUrlLookup((connectionId) => remoteConnectionsStore.getBaseUrl(connectionId));
