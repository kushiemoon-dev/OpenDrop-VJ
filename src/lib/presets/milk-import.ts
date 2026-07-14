/**
 * Runtime MilkDrop preset (.milk/.prjm) → Butterchurn JSON conversion, for
 * drag-and-drop import. Uses jberg's milkdrop-preset-converter — the same
 * tool that produced the bundled 16k preset megapack at build time (see
 * scripts/build-presets.mjs), just run in the browser instead of offline.
 * Dynamically imported: this pulls in an EEL-expression parser + HLSL
 * converter (~1.5MB unpacked) that most sessions never touch.
 */

/** True if a filename looks like a MilkDrop preset (case-insensitive .milk/.prjm). */
export function isMilkPresetFilename(filename: string): boolean {
	return /\.(milk|prjm)$/i.test(filename);
}

/**
 * Convert raw .milk/.prjm file text into Butterchurn preset JSON
 * (init_eqs_str/frame_eqs_str/pixel_eqs_str/shapes/waves/...).
 * Throws if the text has no `[preset00]` header — the underlying converter
 * doesn't validate this itself and would otherwise silently return an
 * empty/blank preset instead of failing.
 */
export async function convertMilkPreset(text: string): Promise<object> {
	if (!text.includes('[preset00]')) {
		throw new Error('Not a MilkDrop preset — missing [preset00] header.');
	}
	const { convertPreset } = await import('milkdrop-preset-converter');
	return convertPreset(text);
}
