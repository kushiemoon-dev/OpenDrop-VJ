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
			v4l2Start: () => Promise<{ ok: boolean; error?: string }>;
			v4l2Stop: () => Promise<{ ok: boolean }>;
			spoutStart: (name: string) => Promise<{ ok: boolean; error?: string }>;
			spoutStop: () => Promise<{ ok: boolean }>;
			listOutputDevices: () => Promise<{ ok: boolean; devices: Array<{ id: number; name: string; maxInputChannels: number; maxOutputChannels: number; defaultSampleRate: number }>; error?: string }>;
			startLoopback: (deviceId: number) => Promise<{ ok: boolean; sampleRate?: number; channels?: number; error?: string }>;
			stopLoopback: () => Promise<{ ok: boolean }>;
			onLoopbackData: (cb: (data: { sampleRate: number; channels: number; pcm: Uint8Array }) => void) => () => void;
			sendAudioFrame: (data: { sampleRate: number; channels: number; pcm: Int16Array }) => void;
			onAudioFrame: (cb: (data: { sampleRate: number; channels: number; pcm: Int16Array }) => void) => () => void;
		};
	}
}

export {};
