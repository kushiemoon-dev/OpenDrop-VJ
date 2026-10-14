use crate::theme;
use glow::HasContext;
use opendrop_engine::compositor::{Compositor, OverlayBlendMode, OverlayLayerInput, COMP_H};
use opendrop_engine::overlay_texture;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One overlay's cached GL texture. `key` is a fingerprint of everything the pixels depend on: the
/// sprite's file path, or the text's content/font/size/color, so the
/// texture is rebuilt exactly when one of those changes, and never
/// per-frame. `texture` is `None` for an overlay whose build failed
/// (unreadable file, undecodable image, a string past the texture limit):
/// the entry is still cached, under the same key, so the failure is not
/// retried every frame until the user actually changes something.
pub(crate) struct OverlayTextureEntry {
    key: String,
    texture: Option<(glow::NativeTexture, u32, u32)>,
}

/// The two vendored faces (`app/assets/fonts/`) available
/// to overlay text rasterization. `FontFamily` has 5 variants, ported
/// verbatim from the web's CSS font stacks, but only Sans and Mono have a
/// face bundled here; Serif/Impact/Comic fall back to Inter, which the
/// panel says out loud. Bundling a serif/display/comic face is a
/// vendoring + licensing decision, deliberately out of this step's scope.
pub(crate) fn overlay_font_bytes(family: opendrop_core::overlay::FontFamily) -> &'static [u8] {
    match family {
        opendrop_core::overlay::FontFamily::Mono => theme::fonts::JETBRAINS_MONO_VARIABLE,
        _ => theme::fonts::INTER_VARIABLE,
    }
}

/// The cache key described on `OverlayTextureEntry::key`.
pub(crate) fn overlay_texture_key(overlay: &opendrop_core::overlay::Overlay, asset: Option<&Path>) -> String {
    match overlay.kind {
        opendrop_core::overlay::OverlayKind::Media => {
            format!("media\u{1}{}", asset.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
        }
        opendrop_core::overlay::OverlayKind::Text => format!(
            "text\u{1}{}\u{1}{:?}\u{1}{}\u{1}{}",
            overlay.text, overlay.font_family, overlay.font_size, overlay.color
        ),
    }
}

/// Builds (or rebuilds) every overlay's texture whose cache key changed,
/// and drops the textures of overlays that no longer exist. Runs once per
/// render tick, right before the overlay composite pass, while the main
/// context is current, since `gl.delete_texture`/`upload_rgba` both require
/// that.
///
/// Text is rasterized at `font_size` vh of the composite frame, which is
/// how the web sized it (`font-size: {ov.fontSize}vh`); the sprite's
/// on-screen size is then the compositor's business
/// (`overlay_quad_half_size_px`), same for both kinds.
pub(crate) fn sync_overlay_textures(
    gl: &glow::Context,
    store: &opendrop_core::overlay::OverlayStore,
    assets: &HashMap<String, PathBuf>,
    textures: &mut HashMap<String, OverlayTextureEntry>,
) {
    textures.retain(|id, entry| {
        let still_there = store.overlays.iter().any(|o| &o.id == id);
        if !still_there {
            if let Some((tex, _, _)) = entry.texture {
                unsafe { gl.delete_texture(tex) };
            }
        }
        still_there
    });

    for overlay in &store.overlays {
        let asset = assets.get(&overlay.id).map(PathBuf::as_path);
        let key = overlay_texture_key(overlay, asset);
        if textures.get(&overlay.id).is_some_and(|e| e.key == key) {
            continue;
        }
        if let Some(old) = textures.remove(&overlay.id) {
            if let Some((tex, _, _)) = old.texture {
                unsafe { gl.delete_texture(tex) };
            }
        }
        let texture = match build_overlay_texture(gl, overlay, asset) {
            Ok(built) => Some(built),
            Err(e) => {
                eprintln!("[app] overlay '{}' ({}): {e}", overlay.name, overlay.id);
                None
            }
        };
        textures.insert(overlay.id.clone(), OverlayTextureEntry { key, texture });
    }
}

/// Decodes/rasterizes one overlay's pixels and uploads them. Split out of
/// `sync_overlay_textures` so the error path there is a single `match`.
pub(crate) fn build_overlay_texture(
    gl: &glow::Context,
    overlay: &opendrop_core::overlay::Overlay,
    asset: Option<&Path>,
) -> Result<(glow::NativeTexture, u32, u32), String> {
    let image = match overlay.kind {
        opendrop_core::overlay::OverlayKind::Media => {
            let path = asset.ok_or("no source file recorded for this sprite")?;
            let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
            overlay_texture::decode_image(&bytes)?
        }
        opendrop_core::overlay::OverlayKind::Text => overlay_texture::rasterize_text(
            overlay_font_bytes(overlay.font_family),
            &overlay.text,
            // `font_size` is in vh, exactly as the web stored it. 1 vh is
            // 1% of the composite frame's height.
            (overlay.font_size / 100.0 * COMP_H as f64) as f32,
            opendrop_core::overlay::parse_hex_color(&overlay.color),
        )?,
    };
    let (w, h) = (image.width, image.height);
    let tex = overlay_texture::upload_rgba(gl, &image)?;
    Ok((tex, w, h))
}

/// Draws every currently-visible overlay into the composite FBO. Called
/// once per tick from `about_to_wait`, inside the compositor's
/// `begin_frame`/`end_frame` bracket and after the strobe pass; see
/// `Compositor::composite_overlay`'s doc comment for why that ordering.
///
/// "Visible" is `core::overlay::visible_overlay_ids`: every un-queued
/// overlay, plus the single active one from the queue rotation.
pub(crate) fn render_overlays(
    gl: &glow::Context,
    compositor: &mut Compositor,
    store: &opendrop_core::overlay::OverlayStore,
    textures: &HashMap<String, OverlayTextureEntry>,
    elapsed_sec: f64,
    beat_pulse: bool,
) {
    if store.overlays.is_empty() {
        return;
    }
    let visible = opendrop_core::overlay::visible_overlay_ids(&store.overlays, store.queue_index);
    for overlay in &store.overlays {
        if !visible.contains(overlay.id.as_str()) {
            continue;
        }
        let Some(&(texture, tex_w, tex_h)) = textures.get(&overlay.id).and_then(|e| e.texture.as_ref()) else {
            continue; // not built yet, or its build failed; already logged
        };
        let transform = opendrop_core::overlay::overlay_transform_at(overlay, elapsed_sec, beat_pulse);
        compositor.composite_overlay(
            gl,
            &OverlayLayerInput {
                texture,
                tex_w,
                tex_h,
                x: transform.x as f32,
                y: transform.y as f32,
                scale: transform.scale as f32,
                rotation_deg: transform.rotation_deg as f32,
                opacity: overlay.opacity as f32,
                blend_mode: OverlayBlendMode::from_css(&overlay.blend_mode),
            },
        );
    }
}
