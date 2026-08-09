// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
declare global {
	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}

	interface Window {
		electronAPI?: {
			isElectron: true;
			getPlatform: () => Promise<string>;
			sendBroadcast: (data: unknown) => void;
			onBroadcast: (cb: (data: unknown) => void) => () => void;
			ndiStart: (name: string, width: number, height: number) => Promise<{ ok: boolean; error?: string }>;
			ndiStop: () => Promise<{ ok: boolean }>;
			ndiFind: () => Promise<{ ok: boolean; sources: Array<{ name: string; urlAddress: string }>; error?: string }>;
			ndiReceiveStart: (name: string, urlAddress: string) => Promise<{ ok: boolean; error?: string }>;
			ndiReceiveStop: () => Promise<{ ok: boolean }>;
			onNdiFrame: (cb: (frame: { width: number; height: number; lineStrideBytes: number; data: Uint8Array<ArrayBuffer> }) => void) => () => void;
			ndiDeckStart: (slot: number, name: string) => Promise<{ ok: boolean; error?: string }>;
			ndiDeckStop: (slot: number) => Promise<{ ok: boolean }>;
			sendDeckFrame: (slot: number, width: number, height: number, buffer: ArrayBuffer) => void;
			v4l2Start: () => Promise<{ ok: boolean; error?: string }>;
			v4l2Stop: () => Promise<{ ok: boolean }>;
			recordStart: () => Promise<{ ok: boolean; path?: string; error?: string }>;
			recordStop: () => Promise<{ ok: boolean; path?: string }>;
			spoutStart: (name: string) => Promise<{ ok: boolean; error?: string }>;
			spoutStop: () => Promise<{ ok: boolean }>;
			listOutputDevices: () => Promise<{ ok: boolean; devices: Array<{ id: number; name: string; maxInputChannels: number; maxOutputChannels: number; defaultSampleRate: number }>; error?: string }>;
			startLoopback: (deviceId: number) => Promise<{ ok: boolean; sampleRate?: number; channels?: number; error?: string }>;
			stopLoopback: () => Promise<{ ok: boolean }>;
			onLoopbackData: (cb: (data: { sampleRate: number; channels: number; pcm: Uint8Array }) => void) => () => void;
			sendAudioFrame: (data: { sampleRate: number; channels: number; pcm: Int16Array }) => void;
			onAudioFrame: (cb: (data: { sampleRate: number; channels: number; pcm: Int16Array }) => void) => () => void;
			startLink: (bpm: number) => Promise<{ ok: boolean; tempo?: number; error?: string }>;
			stopLink: () => Promise<{ ok: boolean }>;
			setLinkTempo: (bpm: number) => Promise<{ ok: boolean; error?: string }>;
			onLinkState: (cb: (state: { tempo: number; beat: number; phase: number; peers: number }) => void) => () => void;
			startOsc: (port: number) => Promise<{ ok: boolean; port?: number; error?: string }>;
			stopOsc: () => Promise<{ ok: boolean }>;
			onOscMsg: (cb: (cmdId: string, value01: number) => void) => () => void;
			startRemote: () => Promise<{ ok: boolean; port?: number; ip?: string; token?: string; error?: string }>;
			stopRemote: () => Promise<{ ok: boolean }>;
			onRemoteCmd: (cb: (cmd: string, value: number) => void) => () => void;
			listScreens: () => Promise<Array<{ id: number; label: string; isPrimary: boolean; bounds: { x: number; y: number; width: number; height: number } }>>;
			openOutputOnDisplay: (displayId: number | null) => Promise<{ ok: boolean; error?: string }>;
			onOutputWindowClosed: (cb: () => void) => () => void;
			hasSecret: (key: string) => Promise<boolean>;
			setSecret: (key: string, value: string) => Promise<{ ok: boolean; error?: string }>;
			clearSecret: (key: string) => Promise<{ ok: boolean; error?: string }>;
			obsConnect: (host: string, port: number) => Promise<{ ok: boolean; error?: string }>;
			obsDisconnect: () => Promise<{ ok: boolean }>;
			obsGetScenes: () => Promise<{ ok: boolean; scenes?: string[]; error?: string }>;
			obsSetScene: (sceneName: string) => Promise<{ ok: boolean; error?: string }>;
			onObsSceneChanged: (cb: (sceneName: string) => void) => () => void;
			obsStartRecord: () => Promise<{ ok: boolean; error?: string }>;
			obsStopRecord: () => Promise<{ ok: boolean; error?: string }>;
			obsGetRecordStatus: () => Promise<{ ok: boolean; recording?: boolean; error?: string }>;
			onObsRecordStateChanged: (cb: (recording: boolean) => void) => () => void;
			twitchConnect: (channel: string) => Promise<{ ok: boolean; error?: string }>;
			twitchDisconnect: () => Promise<{ ok: boolean }>;
			kickConnect: (channel: string) => Promise<{ ok: boolean; error?: string }>;
			kickDisconnect: () => Promise<{ ok: boolean }>;
			onChatMessage: (cb: (msg: { platform: 'twitch' | 'kick'; userId: string; username: string; content: string }) => void) => () => void;
		};
	}
}

export {};
