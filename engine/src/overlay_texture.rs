//! Pixel sources for the compositor's overlay pass (Step 12 of the Phase 8
//! VJ-panels plan): decode a sprite file, rasterize a string, upload either
//! one as a `glow` texture.
//!
//! The web had neither of these: an overlay was a DOM `<img>`/`<div>` the
//! browser laid out, rasterized and blended over the visualizer canvas.
//! Nothing of that survives a native port: the real output (preview,
//! output window, NDI, v4l2) is a GL blit/readback of the compositor's own
//! framebuffer, never an egui draw, so an overlay has to become a texture
//! the compositor can draw (see `compositor::Compositor::composite_overlay`).
//!
//! Both producers return a plain [`RgbaImage`] rather than uploading
//! directly, so the decode/layout half is testable with no GL context at
//! all; [`upload_rgba`] is the only function here that needs one.
//!
//! Nothing in this module caches: an overlay's texture is rebuilt only when
//! its content/font/size/color (or its file) changes, and tracking that is
//! the caller's job (`app::AppState::overlay_textures`).

use ab_glyph::{point, Font, FontRef, Glyph, GlyphId, Point, PxScale, ScaleFont};
use glow::HasContext;

/// Straight (NOT premultiplied) 8-bit RGBA, row 0 = top row, the layout
/// both `image`'s decoders and [`rasterize_text`] produce natively, and the
/// one the overlay fragment shader expects (it flips V at the vertex stage,
/// see `compositor::OVERLAY_VERTEX_SRC`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

/// Ceiling on either dimension of anything uploaded through this module.
/// A rasterized string is sized by the user's font-size slider times the
/// string's length, so it is genuinely unbounded from the UI's side; GL's
/// own `GL_MAX_TEXTURE_SIZE` is only guaranteed to be 2048 on GLES 3.0, and
/// every driver this app targets is well past 4096. Anything bigger is a
/// mistake (or a paste of a whole document into the text box), so it is
/// refused with a message the panel can show rather than silently
/// allocating hundreds of MB.
pub const MAX_TEXTURE_DIM: u32 = 4096;

/// Decodes a PNG/JPEG/GIF/BMP/WebP sprite into straight RGBA8.
pub fn decode_image(bytes: &[u8]) -> Result<RgbaImage, String> {
    let decoded = image::load_from_memory(bytes).map_err(|e| format!("image decode failed: {e}"))?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return Err("image decoded to a zero-sized buffer".to_string());
    }
    if width > MAX_TEXTURE_DIM || height > MAX_TEXTURE_DIM {
        return Err(format!("image is {width}x{height}, larger than the {MAX_TEXTURE_DIM} px limit"));
    }
    Ok(RgbaImage { width, height, pixels: rgba.into_raw() })
}

/// Rasterizes `text` at `px_size` pixels into a tightly-cropped RGBA
/// buffer: every pixel carries `color` in RGB and the glyph's antialiasing
/// coverage in A, so the result drops straight into the same sprite path
/// `decode_image`'s output takes (Override 3 of the plan: no glyph atlas,
/// one texture per string, rebuilt only when the string/font/size/color
/// changes).
///
/// `\n` starts a new line; lines are laid out left-aligned one
/// `ascent - descent + line_gap` apart, with horizontal kerning applied.
/// A string that rasterizes to nothing (empty, or all whitespace) yields a
/// 1x1 fully transparent image rather than an error. That is a legitimate
/// state for a text overlay whose content box the user just cleared.
pub fn rasterize_text(font_bytes: &[u8], text: &str, px_size: f32, color: [u8; 3]) -> Result<RgbaImage, String> {
    if !(px_size.is_finite() && px_size > 0.0) {
        return Err(format!("non-positive font size: {px_size}"));
    }
    let font = FontRef::try_from_slice(font_bytes).map_err(|e| format!("font parse failed: {e}"))?;
    let scaled = font.as_scaled(PxScale::from(px_size));

    let glyphs = layout_glyphs(&scaled, text, px_size);
    let outlined: Vec<_> = glyphs.into_iter().filter_map(|g| font.outline_glyph(g)).collect();

    // Union of every glyph's rasterized extent, tighter than the font's
    // nominal line box, and the reason the quad the compositor builds is
    // exactly the visible ink rather than ink plus slack.
    let mut min = point(f32::MAX, f32::MAX);
    let mut max = point(f32::MIN, f32::MIN);
    for g in &outlined {
        let b = g.px_bounds();
        min.x = min.x.min(b.min.x);
        min.y = min.y.min(b.min.y);
        max.x = max.x.max(b.max.x);
        max.y = max.y.max(b.max.y);
    }
    if outlined.is_empty() || max.x <= min.x || max.y <= min.y {
        return Ok(RgbaImage { width: 1, height: 1, pixels: vec![0, 0, 0, 0] });
    }

    let width = (max.x - min.x).ceil() as u32;
    let height = (max.y - min.y).ceil() as u32;
    if width > MAX_TEXTURE_DIM || height > MAX_TEXTURE_DIM {
        return Err(format!(
            "text rasterizes to {width}x{height}, larger than the {MAX_TEXTURE_DIM} px limit; shorten it or lower the size"
        ));
    }

    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    for g in &outlined {
        let bounds = g.px_bounds();
        let (ox, oy) = (bounds.min.x - min.x, bounds.min.y - min.y);
        g.draw(|gx, gy, coverage| {
            let x = ox as i32 + gx as i32;
            let y = oy as i32 + gy as i32;
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                return;
            }
            let i = (y as usize * width as usize + x as usize) * 4;
            // Glyphs can overlap (kerned pairs, accents); keep the
            // strongest coverage rather than letting a later glyph's
            // transparent edge erase an earlier glyph's solid pixel.
            let a = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
            if a > pixels[i + 3] {
                pixels[i] = color[0];
                pixels[i + 1] = color[1];
                pixels[i + 2] = color[2];
                pixels[i + 3] = a;
            }
        });
    }
    Ok(RgbaImage { width, height, pixels })
}

/// Pen-walks `text` into positioned glyphs. Split out of `rasterize_text`
/// so the layout half stays readable next to the rasterization half.
fn layout_glyphs<F, SF>(scaled: &SF, text: &str, px_size: f32) -> Vec<Glyph>
where
    F: Font,
    SF: ScaleFont<F>,
{
    let line_height = scaled.height() + scaled.line_gap();
    let mut glyphs = Vec::new();
    let mut caret: Point = point(0.0, scaled.ascent());
    let mut previous: Option<GlyphId> = None;
    for c in text.chars() {
        if c == '\n' {
            caret = point(0.0, caret.y + line_height);
            previous = None;
            continue;
        }
        if c.is_control() {
            continue;
        }
        let id = scaled.glyph_id(c);
        if let Some(prev) = previous {
            caret.x += scaled.kern(prev, id);
        }
        previous = Some(id);
        glyphs.push(id.with_scale_and_position(px_size, caret));
        caret.x += scaled.h_advance(id);
    }
    glyphs
}

/// Uploads an [`RgbaImage`] as a fresh `GL_TEXTURE_2D`, clamped and
/// linearly filtered, the same parameters `deck::create_shared_deck_texture`
/// sets, except that this one actually ships pixel data (that one allocates
/// an empty target for projectM to copy into).
///
/// Must run while the context the returned texture will be sampled from is
/// current. The caller owns the handle from here on and must
/// `gl.delete_texture` it (`app`'s overlay cache does, on eviction).
///
/// # Safety-adjacent note
/// Sets `GL_UNPACK_ALIGNMENT` to 1 and leaves it there: rows are `width*4`
/// bytes, always 4-byte aligned, so the default of 4 would work for RGBA.
/// But a `width` that made a row unaligned would silently skew the image,
/// and every other upload path in this app (egui_glow's own included) sets
/// this itself before uploading.
pub fn upload_rgba(gl: &glow::Context, img: &RgbaImage) -> Result<glow::NativeTexture, String> {
    let expected = img.width as usize * img.height as usize * 4;
    if img.pixels.len() != expected {
        return Err(format!(
            "pixel buffer is {} bytes, expected {expected} for {}x{}",
            img.pixels.len(),
            img.width,
            img.height
        ));
    }
    unsafe {
        let tex = gl.create_texture().map_err(|e| format!("create_texture (overlay) failed: {e}"))?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            img.width as i32,
            img.height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&img.pixels)),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(tex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one font this crate's tests can rely on being present: the
    /// UI font `app` already vendors (`app/assets/fonts/`, Phase 7 Step 2).
    /// Referenced across the crate boundary on purpose: duplicating an
    /// 862 KB .ttf into `engine/` just for tests would be worse.
    const TEST_FONT: &[u8] = include_bytes!("../../app/assets/fonts/Inter-Variable.ttf");

    mod decode_image {
        use super::*;

        /// Smallest valid PNG this test can carry inline: 2x2, built by
        /// `image` itself so the fixture can't drift from the decoder.
        fn png_2x2() -> Vec<u8> {
            let mut buf = std::io::Cursor::new(Vec::new());
            let img = image::RgbaImage::from_fn(2, 2, |x, y| {
                image::Rgba([(x * 255) as u8, (y * 255) as u8, 0, 255])
            });
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut buf, image::ImageFormat::Png)
                .expect("encoding a 2x2 PNG should work");
            buf.into_inner()
        }

        #[test]
        fn decodes_a_png_to_straight_rgba_with_row_0_on_top() {
            let decoded = decode_image(&png_2x2()).expect("2x2 PNG should decode");
            assert_eq!((decoded.width, decoded.height), (2, 2));
            assert_eq!(decoded.pixels.len(), 2 * 2 * 4);
            // (0,0) is the top-left texel: x=0, y=0 -> rgba(0, 0, 0, 255).
            assert_eq!(&decoded.pixels[0..4], &[0, 0, 0, 255]);
            // (1,0): x=1 -> red 255.
            assert_eq!(&decoded.pixels[4..8], &[255, 0, 0, 255]);
            // (0,1): y=1 -> green 255.
            assert_eq!(&decoded.pixels[8..12], &[0, 255, 0, 255]);
        }

        #[test]
        fn rejects_bytes_that_are_not_an_image() {
            let err = decode_image(b"not an image at all").unwrap_err();
            assert!(err.contains("image decode failed"), "unexpected error: {err}");
        }
    }

    mod rasterize_text {
        use super::*;

        #[test]
        fn produces_a_buffer_matching_its_declared_dimensions() {
            let img = rasterize_text(TEST_FONT, "Hello", 32.0, [255, 0, 128]).expect("rasterizing should work");
            assert_eq!(img.pixels.len(), img.width as usize * img.height as usize * 4);
            assert!(img.width > 0 && img.height > 0);
        }

        #[test]
        fn every_inked_pixel_carries_the_requested_color() {
            let img = rasterize_text(TEST_FONT, "Hello", 32.0, [255, 0, 128]).expect("rasterizing should work");
            let inked: Vec<&[u8; 4]> = img.pixels.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).collect();
            assert!(!inked.is_empty(), "'Hello' at 32px must produce ink");
            for p in inked {
                assert_eq!(&p[0..3], &[255, 0, 128]);
            }
        }

        #[test]
        fn scales_with_the_pixel_size() {
            let small = rasterize_text(TEST_FONT, "Hello", 16.0, [255; 3]).unwrap();
            let large = rasterize_text(TEST_FONT, "Hello", 64.0, [255; 3]).unwrap();
            assert!(large.width > small.width * 2, "{} vs {}", large.width, small.width);
            assert!(large.height > small.height * 2, "{} vs {}", large.height, small.height);
        }

        #[test]
        fn a_newline_stacks_lines_instead_of_widening_one() {
            let one_line = rasterize_text(TEST_FONT, "AB", 32.0, [255; 3]).unwrap();
            let two_lines = rasterize_text(TEST_FONT, "A\nB", 32.0, [255; 3]).unwrap();
            assert!(two_lines.height > one_line.height);
            assert!(two_lines.width < one_line.width);
        }

        #[test]
        fn an_empty_or_blank_string_is_a_1x1_transparent_pixel_not_an_error() {
            for text in ["", "   ", "\n\n"] {
                let img = rasterize_text(TEST_FONT, text, 32.0, [255; 3]).unwrap_or_else(|e| panic!("{text:?}: {e}"));
                assert_eq!((img.width, img.height), (1, 1));
                assert_eq!(img.pixels, vec![0, 0, 0, 0]);
            }
        }

        #[test]
        fn rejects_a_non_positive_size() {
            assert!(rasterize_text(TEST_FONT, "x", 0.0, [255; 3]).is_err());
            assert!(rasterize_text(TEST_FONT, "x", -4.0, [255; 3]).is_err());
            assert!(rasterize_text(TEST_FONT, "x", f32::NAN, [255; 3]).is_err());
        }

        #[test]
        fn rejects_a_string_that_would_blow_past_the_texture_limit() {
            let long = "W".repeat(4000);
            let err = rasterize_text(TEST_FONT, &long, 64.0, [255; 3]).unwrap_err();
            assert!(err.contains("larger than the"), "unexpected error: {err}");
        }

        #[test]
        fn rejects_bytes_that_are_not_a_font() {
            assert!(rasterize_text(b"nope", "x", 32.0, [255; 3]).is_err());
        }
    }
}
