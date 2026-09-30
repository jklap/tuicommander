/**
 * The microphone and the speaker of a browser tab, on a WebSocket.
 *
 * Transport only, deliberately. Utterance boundaries, the activation phrase,
 * the hold-back and whether a turn may be delivered are all decided in Rust,
 * exactly as they are for the desktop endpoint — this file captures samples,
 * ships them, and plays back what comes the other way. Anything here that
 * started deciding *when* someone had finished speaking would be a second
 * implementation of the segmenter, disagreeing with the first one only on
 * browsers.
 *
 * ## Wire format
 *
 * | Direction | Frame | Meaning |
 * |---|---|---|
 * | up | binary | `f32` little-endian samples, mono, 16 kHz |
 * | up | `{"type":"playback-ended"}` | the reply finished playing |
 * | down | binary | `u32` little-endian sample rate, then `f32` samples |
 * | down | `{"type":"stop"}` | stop playing and drop the queue |
 *
 * Resampling happens here rather than on the server: the `AudioContext` is
 * created at 16 kHz and resamples the device's native rate for free, and
 * sending 48 kHz would triple the bytes for audio the segmenter throws away.
 */

import { appLogger } from "../stores/appLogger";

/** The rate Whisper wants, and therefore the rate the socket carries. */
const CAPTURE_SAMPLE_RATE = 16_000;

/**
 * How many samples are buffered before a frame is sent.
 *
 * 1024 at 16 kHz is 64 ms — short enough that the hold-back and the
 * activation phrase still feel immediate, long enough that a conversation is
 * ~16 frames a second rather than a frame per audio callback.
 */
const FRAME_SAMPLES = 1024;

/** A live browser audio session: its socket, its microphone and its playback. */
export interface BrowserVoiceSession {
	/** Close the socket, the microphone and anything still playing. */
	stop(): void;
}

/** What `connectBrowserVoice` needs, named so a test can supply its own. */
export interface BrowserVoiceDeps {
	/** Build the socket. Injected because a test has no server. */
	openSocket(url: string): WebSocket;
	/** Open the microphone. */
	getUserMedia(): Promise<MediaStream>;
	/** Build the capture context, at the rate the socket carries. */
	createContext(sampleRate: number): AudioContext;
}

const browserDeps: BrowserVoiceDeps = {
	openSocket: (url) => new WebSocket(url),
	getUserMedia: () =>
		navigator.mediaDevices.getUserMedia({
			audio: {
				// The browser's own cancellation, on top of the server's. They
				// are solving the same problem at different distances: this one
				// knows the device, and AEC3 in Rust knows what was sent to be
				// played. Neither is sufficient alone on a laptop speaker.
				echoCancellation: true,
				noiseSuppression: true,
				autoGainControl: true,
			},
		}),
	createContext: (sampleRate) => new AudioContext({ sampleRate }),
};

/** The `ws://`/`wss://` audio socket for this origin. */
export function audioSocketUrl(owner: string, origin = window.location): string {
	const scheme = origin.protocol === "https:" ? "wss:" : "ws:";
	return `${scheme}//${origin.host}/dictation/hands-free/audio?owner=${encodeURIComponent(owner)}`;
}

/**
 * Split a binary downlink frame into its rate and its samples.
 *
 * Exported for its own test: a frame that is decoded at the wrong rate plays
 * at the wrong pitch, which is the kind of failure that is obvious to a human
 * and invisible to an assertion about lengths.
 */
export function decodeReply(frame: ArrayBuffer): {
	sampleRate: number;
	samples: Float32Array;
} {
	const view = new DataView(frame);
	return {
		sampleRate: view.getUint32(0, true),
		samples: new Float32Array(frame.slice(4)),
	};
}

/** Pack captured samples for the uplink. */
export function encodeCapture(samples: Float32Array): ArrayBuffer {
	// `Float32Array` is already little-endian on every platform a browser runs
	// on, so the buffer is the frame.
	return samples.buffer.slice(samples.byteOffset, samples.byteOffset + samples.byteLength) as ArrayBuffer;
}

/**
 * Open this tab's microphone and speaker for the conversation owned by
 * `owner`.
 *
 * The socket is opened **before** arming, and arming fails when it is not
 * there: Rust refuses an owner with no client rather than falling back to its
 * own microphone, so a tab that armed first would be told its own name is
 * unknown.
 */
export async function connectBrowserVoice(
	owner: string,
	deps: BrowserVoiceDeps = browserDeps,
): Promise<BrowserVoiceSession> {
	const socket = deps.openSocket(audioSocketUrl(owner));
	socket.binaryType = "arraybuffer";

	const stream = await deps.getUserMedia();
	const context = deps.createContext(CAPTURE_SAMPLE_RATE);
	const source = context.createMediaStreamSource(stream);
	// `ScriptProcessorNode` rather than an `AudioWorklet`: a worklet needs a
	// separate module URL, which the packaged frontend serves from a path the
	// plugin loader also owns, and this node is deprecated rather than absent.
	// The work per callback is a copy, not a computation.
	const capture = context.createScriptProcessor(FRAME_SAMPLES, 1, 1);

	capture.onaudioprocess = (event) => {
		if (socket.readyState !== WebSocket.OPEN) return;
		socket.send(encodeCapture(event.inputBuffer.getChannelData(0)));
	};
	source.connect(capture);
	// A `ScriptProcessorNode` only runs while it is connected to a
	// destination. Nothing is *heard* from it — it emits the silence its
	// output buffer was created with — but without this the callback above
	// never fires and the conversation hears nothing.
	capture.connect(context.destination);

	let playing: AudioBufferSourceNode | null = null;
	const stopPlayback = () => {
		playing?.stop();
		playing = null;
	};

	socket.onmessage = (event) => {
		if (typeof event.data === "string") {
			if (JSON.parse(event.data).type === "stop") stopPlayback();
			return;
		}
		const { sampleRate, samples } = decodeReply(event.data as ArrayBuffer);
		if (!sampleRate || samples.length === 0) return;
		// Its own rate, not the capture context's: the engine renders at 24 kHz
		// and the browser resamples. Reusing the capture rate would play every
		// reply 50% slow.
		const buffer = context.createBuffer(1, samples.length, sampleRate);
		buffer.getChannelData(0).set(samples);
		stopPlayback();
		const node = context.createBufferSource();
		node.buffer = buffer;
		node.connect(context.destination);
		node.onended = () => {
			if (playing !== node) return;
			playing = null;
			// The server cannot see the end of playback, and without this the
			// reply stays `speaking` until its rendered duration elapses.
			if (socket.readyState === WebSocket.OPEN) {
				socket.send(JSON.stringify({ type: "playback-ended" }));
			}
		};
		node.start();
		playing = node;
	};

	socket.onerror = () => {
		appLogger.error("dictation", "Browser voice socket failed");
	};

	return {
		stop() {
			stopPlayback();
			capture.onaudioprocess = null;
			capture.disconnect();
			source.disconnect();
			for (const track of stream.getTracks()) track.stop();
			void context.close();
			socket.close();
		},
	};
}
