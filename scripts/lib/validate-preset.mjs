// Shared by build-presets.mjs and prune-invalid-presets.mjs: a preset that
// fails this same `new Function(...)` check Butterchurn itself runs would
// crash the whole app the moment a user selects it — no per-preset guard
// exists in the load path (this is how the '$$$ Royal - Mashup (157)' bug
// got found: it's presetList[0], auto-selected on every ▶ Start). Filtering
// here means one corrupt preset just gets skipped, never breaks the app.

export function compiles(str) {
	if (typeof str !== 'string' || str === '') return true;
	try { new Function('a', str + ' return a;'); return true; }
	catch { return false; }
}

export function isValid(preset) {
	if (!compiles(preset.init_eqs_str) || !compiles(preset.frame_eqs_str) || !compiles(preset.pixel_eqs_str)) return false;
	for (const s of preset.shapes ?? []) if (!compiles(s.init_eqs_str) || !compiles(s.frame_eqs_str)) return false;
	for (const w of preset.waves ?? []) if (!compiles(w.init_eqs_str) || !compiles(w.frame_eqs_str) || !compiles(w.point_eqs_str)) return false;
	return true;
}
