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
			return { meshWidth: 48, meshHeight: 36, textureRatio: 1, pixelRatio: 1, outputFXAA: true };
		case 'high':
			return { meshWidth: 64, meshHeight: 48, textureRatio: 1.5, pixelRatio: dpr, outputFXAA: true };
	}
}
