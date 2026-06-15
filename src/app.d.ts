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
			getLoopbackSources: () => Promise<{ id: string; name: string }[]>;
			sendBroadcast: (data: unknown) => void;
			onBroadcast: (cb: (data: unknown) => void) => () => void;
		};
	}
}

export {};
