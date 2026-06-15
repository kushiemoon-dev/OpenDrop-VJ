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
		};
	}
}

export {};
