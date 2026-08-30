# Variable Weight Fonts

This directory contains vendored variable-weight fonts for OpenDrop-Native's UI redesign (Phase 7).

Both fonts are distributed under the SIL Open Font License v1.1 (OFL 1.1) with no Reserved Font Names, permitting free use, modification, and redistribution.

## Inter

- **Upstream**: https://github.com/rsms/inter
- **Release**: v4.0
- **Download URL**: https://github.com/rsms/inter/releases/download/v4.0/Inter-4.0.zip
- **File**: `Inter-Variable.ttf`
- **Size**: 862,936 bytes
- **SHA256**: `746431e950fd28d29b0189d708d4a5852a8458edb3184387eadcee9e5e34676c`
- **License**: `Inter-OFL.txt` (SIL Open Font License v1.1)

Inter is a carefully designed typeface family for computer screens. The variable font supports all OpenType weight axes (100-900).

## JetBrains Mono

- **Upstream**: https://github.com/JetBrains/JetBrainsMono
- **Release**: v2.304
- **Download URL**: https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip
- **File**: `JetBrainsMono-Variable.ttf` (extracted from `fonts/variable/JetBrainsMono[wght].ttf`)
- **Size**: 303,144 bytes
- **SHA256**: `662a196d58f1183bf2d77428b6d5283fe3f45161ab021bea4036bc98e5cac016`
- **License**: `JetBrainsMono-OFL.txt` (SIL Open Font License v1.1)

JetBrains Mono is a monospace font designed specifically for code and development environments. The variable font supports weight axis (100-800).

## Verification

To verify the integrity of downloaded fonts:

```bash
sha256sum Inter-Variable.ttf
# Expected: 746431e950fd28d29b0189d708d4a5852a8458edb3184387eadcee9e5e34676c

sha256sum JetBrainsMono-Variable.ttf
# Expected: 662a196d58f1183bf2d77428b6d5283fe3f45161ab021bea4036bc98e5cac016
```

## License

Both fonts are distributed under the SIL Open Font License v1.1. See `Inter-OFL.txt` and `JetBrainsMono-OFL.txt` for the full license text.

### Key Points

- Fonts may be used, modified, and redistributed freely
- Fonts cannot be sold by themselves
- Modified versions must remain under OFL 1.1
- No Reserved Font Names (both Inter and JetBrains Mono use OFL 1.1 without Reserved Font Name restrictions)
- Fonts can be bundled with software for free or commercial distribution

## Integration

These fonts are vendored for use in the OpenDrop-Native UI. Reference via `include_bytes!()` macro when implementing custom font loading in Rust (planned for Phase 7, Step 5+).
