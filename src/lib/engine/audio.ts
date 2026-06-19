/**
 * AudioEngine — manages a single Web Audio context and source.
 *
 * One shared AudioContext feeds an AnalyserNode that is connected
 * to all active decks via Deck.connectAudio(analyser).
 */

export type AudioSourceType = 'none' | 'mic' | 'file' | 'display' | 'loopback';

export interface AudioLevels {
	rms: number; // 0–1 overall RMS
	bass: number;
	mid: number;
	treble: number;
}

export class AudioEngine {
	readonly ctx: AudioContext;
	readonly analyser: AnalyserNode;
	readonly gainNode: GainNode;

	private currentSource: AudioNode | null = null;
	private currentStream: MediaStream | null = null;
	private _sourceType: AudioSourceType = 'none';
	// HTMLMediaElement can only have one MediaElementAudioSourceNode — cache it.
	private mediaElementSource: MediaElementAudioSourceNode | null = null;
	// AudioWorklet for native loopback PCM injection.
	private loopbackNode: AudioWorkletNode | null = null;
	private workletLoaded = false;
	// AudioWorklet for capturing the live signal and streaming it to the output window.
	private captureNode: AudioWorkletNode | null = null;
	private captureSink: GainNode | null = null;
	private captureWorkletLoaded = false;

	private readonly fftData: Uint8Array<ArrayBuffer>;

	get sourceType(): AudioSourceType {
		return this._sourceType;
	}

	constructor() {
		this.ctx = new AudioContext();
		this.analyser = this.ctx.createAnalyser();
		this.analyser.fftSize = 2048;
		this.analyser.smoothingTimeConstant = 0.8;
		this.gainNode = this.ctx.createGain();

		// gainNode → analyser → silentSink → destination
		// silentSink keeps the graph alive (Web Audio won't process nodes with no
		// path to destination) without outputting sound through the speakers.
		this.gainNode.connect(this.analyser);
		const silentSink = this.ctx.createGain();
		silentSink.gain.value = 0;
		this.analyser.connect(silentSink);
		silentSink.connect(this.ctx.destination);

		this.fftData = new Uint8Array(this.analyser.frequencyBinCount) as Uint8Array<ArrayBuffer>;
	}

	/** Resume the AudioContext (required after a user gesture). */
	async resume(): Promise<void> {
		if (this.ctx.state === 'suspended') {
			await this.ctx.resume();
		}
	}

	/** List all available audio input devices (includes PipeWire monitors on Linux). */
	static async listAudioDevices(): Promise<MediaDeviceInfo[]> {
		// Trigger permission prompt so labels are populated
		await navigator.mediaDevices.getUserMedia({ audio: true, video: false }).then(
			(s) => s.getTracks().forEach((t) => t.stop()),
			() => {} // ignore if denied — labels may be empty
		);
		const devices = await navigator.mediaDevices.enumerateDevices();
		return devices.filter((d) => d.kind === 'audioinput');
	}

	/** Connect a specific audio input device by deviceId. */
	async connectDevice(deviceId: string): Promise<void> {
		const stream = await navigator.mediaDevices.getUserMedia({
			audio: { deviceId: { exact: deviceId } },
			video: false
		});
		this._connectStream(stream, 'mic');
	}

	/** Connect the default microphone. */
	async connectMic(): Promise<void> {
		const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
		this._connectStream(stream, 'mic');
	}

	/**
	 * Capture system/tab audio via getDisplayMedia.
	 * Firefox opens a picker — choose a tab and check "Share tab audio",
	 * or choose "Entire screen" and check "Share system audio".
	 * Video track is stopped immediately; only audio is used.
	 */
	async connectDisplay(): Promise<void> {
		const stream = await navigator.mediaDevices.getDisplayMedia({
			audio: true,
			video: true // Firefox requires video:true to open the picker
		});

		// Stop video tracks immediately — we only need audio
		stream.getVideoTracks().forEach((t) => t.stop());

		const audioTracks = stream.getAudioTracks();
		if (audioTracks.length === 0) {
			stream.getTracks().forEach((t) => t.stop());
			throw new Error(
				'No audio captured. In the picker: choose a tab and check "Share tab audio", or choose "Entire screen" and check "Share system audio".'
			);
		}

		// Build an audio-only stream from the captured tracks
		const audioOnlyStream = new MediaStream(audioTracks);
		this._connectStream(audioOnlyStream, 'display');
	}

	/**
	 * Prepare the AudioWorklet for native loopback PCM injection.
	 * Call once before the first pushLoopbackPcm() call; idempotent thereafter.
	 * The renderer must subscribe to IPC loopback:data and call pushLoopbackPcm per chunk.
	 */
	async connectLoopbackPcm(): Promise<void> {
		this._disconnectCurrent();
		if (!this.workletLoaded) {
			await this.ctx.audioWorklet.addModule('/loopback-worklet.js');
			this.workletLoaded = true;
		}
		this.loopbackNode = new AudioWorkletNode(this.ctx, 'loopback-pcm', {
			numberOfInputs: 0,
			numberOfOutputs: 1,
			outputChannelCount: [2],
		});
		this.loopbackNode.connect(this.gainNode);
		this.currentSource = this.loopbackNode;
		this._sourceType = 'loopback';
	}

	/**
	 * Push a PCM chunk (Int16 interleaved) received from the Electron main process
	 * via IPC into the loopback AudioWorklet ring buffer.
	 */
	pushLoopbackPcm(data: { sampleRate: number; channels: number; pcm: Uint8Array }): void {
		if (!this.loopbackNode) return;
		// Convert IPC Uint8Array view to Int16Array — zero-copy reinterpret.
		const i16view = new Int16Array(data.pcm.buffer, data.pcm.byteOffset, data.pcm.byteLength / 2);
		// Copy so we can transfer the buffer (transferring the original would detach the IPC buffer).
		const i16copy = new Int16Array(i16view);
		this.loopbackNode.port.postMessage(
			{ sampleRate: data.sampleRate, channels: data.channels, pcm: i16copy },
			[i16copy.buffer],
		);
	}

	/** Connect an audio file element as the source. */
	connectMediaElement(el: HTMLMediaElement): void {
		this._disconnectCurrent();
		// Reuse the existing node — createMediaElementAudioSource throws if called twice
		// on the same element ("HTMLMediaElement already connected").
		if (!this.mediaElementSource || (this.mediaElementSource as any).mediaElement !== el) {
			this.mediaElementSource = this.ctx.createMediaElementSource(el);
		}
		this.mediaElementSource.connect(this.gainNode);
		this.mediaElementSource.connect(this.ctx.destination);
		this.currentSource = this.mediaElementSource;
		this._sourceType = 'file';
	}

	/** Set master gain (0–1). */
	setGain(value: number): void {
		this.gainNode.gain.setTargetAtTime(value, this.ctx.currentTime, 0.01);
	}

	/**
	 * Sample frequency-domain audio levels.
	 * Call every frame for VU meters.
	 */
	getLevels(): AudioLevels {
		this.analyser.getByteFrequencyData(this.fftData);
		const len = this.fftData.length;

		let sum = 0;
		let bassSum = 0;
		let midSum = 0;
		let trebleSum = 0;

		const bassEnd = Math.floor(len * 0.05);
		const midEnd = Math.floor(len * 0.25);

		for (let i = 0; i < len; i++) {
			const v = this.fftData[i] / 255;
			sum += v;
			if (i < bassEnd) bassSum += v;
			else if (i < midEnd) midSum += v;
			else trebleSum += v;
		}

		return {
			rms: sum / len,
			bass: bassSum / bassEnd,
			mid: midSum / (midEnd - bassEnd),
			treble: trebleSum / (len - midEnd)
		};
	}

	/**
	 * Start capturing the live audio signal (from gainNode) and posting Int16 PCM
	 * chunks to onFrame. Used to stream audio to the output window.
	 * Idempotent — safe to call multiple times (only one capture node is created).
	 */
	async startPcmCapture(onFrame: (data: { sampleRate: number; channels: number; pcm: Int16Array }) => void): Promise<void> {
		if (this.captureNode) return; // already running
		if (!this.captureWorkletLoaded) {
			await this.ctx.audioWorklet.addModule('/capture-worklet.js');
			this.captureWorkletLoaded = true;
		}
		const node = new AudioWorkletNode(this.ctx, 'capture-pcm', {
			numberOfInputs: 1,
			numberOfOutputs: 1, // required — Chromium skips process() on nodes with no output path
		});
		node.port.onmessage = (e) => onFrame(e.data as { sampleRate: number; channels: number; pcm: Int16Array });
		this.gainNode.connect(node); // tap the same signal fed to the analyser
		// Route the (silent) output to destination so Chromium keeps process() alive.
		const sink = this.ctx.createGain();
		sink.gain.value = 0;
		node.connect(sink);
		sink.connect(this.ctx.destination);
		this.captureNode = node;
		this.captureSink = sink;
	}

	/** Stop the PCM capture and release the associated nodes. */
	stopPcmCapture(): void {
		if (!this.captureNode) return;
		try { this.gainNode.disconnect(this.captureNode); } catch { /* already disconnected */ }
		try { this.captureNode.disconnect(); } catch { /* already disconnected */ }
		this.captureNode.port.onmessage = null;
		this.captureNode = null;
		if (this.captureSink) {
			try { this.captureSink.disconnect(); } catch { /* already disconnected */ }
			this.captureSink = null;
		}
	}

	/**
	 * Push a PCM chunk received from the capture-worklet relay (already Int16Array)
	 * into the loopback AudioWorklet ring buffer. Distinct from pushLoopbackPcm which
	 * expects a Uint8Array (RtAudio byte buffer) and re-interprets it.
	 */
	pushCapturePcm(data: { sampleRate: number; channels: number; pcm: Int16Array }): void {
		if (!this.loopbackNode) return;
		// data.pcm is already an Int16Array — copy so we can transfer the buffer.
		const i16copy = new Int16Array(data.pcm);
		this.loopbackNode.port.postMessage(
			{ sampleRate: data.sampleRate, channels: data.channels, pcm: i16copy },
			[i16copy.buffer],
		);
	}

	/** Stop the current audio source and release the stream. */
	disconnect(): void {
		this._disconnectCurrent();
		this._sourceType = 'none';
	}

	destroy(): void {
		this.stopPcmCapture();
		this.disconnect();
		this.ctx.close();
	}

	private _connectStream(stream: MediaStream, type: AudioSourceType): void {
		this._disconnectCurrent();
		const source = this.ctx.createMediaStreamSource(stream);
		source.connect(this.gainNode);
		this.currentSource = source;
		this.currentStream = stream;
		this._sourceType = type;
	}

	private _disconnectCurrent(): void {
		if (this.currentSource) {
			try {
				this.currentSource.disconnect();
			} catch {
				// already disconnected
			}
			this.currentSource = null;
		}
		// loopbackNode is always stored as currentSource — null it here too.
		this.loopbackNode = null;
		if (this.currentStream) {
			this.currentStream.getTracks().forEach((t) => t.stop());
			this.currentStream = null;
		}
	}
}
