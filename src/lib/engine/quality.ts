export type QualityTier = 'low' | 'medium' | 'high';

export interface QualitySettings {
	meshWidth: number;
	meshHeight: number;
	textureRatio: number;
	pixelRatio: number;
	outputFXAA: boolean;
}

export const DEFAULT_TIER: QualityTier = 'medium';

export function getQualitySettings(tier: QualityTier): QualitySettings {
	const dpr = typeof window !== 'undefined' ? Math.min(window.devicePixelRatio || 1, 2) : 1;
	switch (tier) {
		case 'low':
			return { meshWidth: 32, meshHeight: 24, textureRatio: 1, pixelRatio: 1, outputFXAA: false };
		case 'medium':
			// FXAA disabled: butterchurn 2.6.7 FXAA pass produces visual noise artifacts.
			return { meshWidth: 48, meshHeight: 36, textureRatio: 1, pixelRatio: 1, outputFXAA: false };
		case 'high':
			// textureRatio kept at 1: values >1 multiply internal buffer to pixelRatio*textureRatio
			// (e.g. 1280*2*1.5 = 3840-wide buffer), causing severe GPU performance regression.
			// FXAA disabled for same reason as medium.
			return { meshWidth: 64, meshHeight: 48, textureRatio: 1, pixelRatio: dpr, outputFXAA: false };
	}
}

export type InvisibleMode = 'eco' | 'pause' | 'off';

export interface PerfSettings {
	targetFps: number;      // FPS cible du rendu (30 | 45 | 60)
	invisibleMode: InvisibleMode;  // comportement des decks invisibles
	invisibleFps: number;   // FPS des decks éco (~8)
}

export const DEFAULT_PERF: PerfSettings = {
	targetFps: 45,
	invisibleMode: 'eco',
	invisibleFps: 8
};
