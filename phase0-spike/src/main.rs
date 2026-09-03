//! Phase 0 spike (throwaway): see /srv/http/OpenDrop-Native/PLAN.md, Phase 0 section.
//! Proves out: home-grown bindgen plus the system libprojectM, 4 EGL pbuffer contexts
//! sharing one share group (one per deck), glCopyTexSubImage2D into a shared texture,
//! a compositor in a 5th context, GL state save/restore around each render_frame, and
//! measures ms/frame plus the preset compat rate.

#[allow(non_camel_case_types, non_snake_case, non_upper_case_globals, dead_code)]
mod ffi {
    include!(concat!(env!("OUT_DIR"), "/projectm_bindings.rs"));
}

use glow::HasContext;
use khronos_egl as egl;
use std::ffi::{c_char, c_void, CStr, CString};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

type Egl = egl::DynamicInstance<egl::Latest>;

const DECK_W: i32 = 640;
const DECK_H: i32 = 360;
const DECK_COUNT: usize = 4;
const COMP_W: i32 = DECK_W * 2;
const COMP_H: i32 = DECK_H * 2;
const WARMUP_FRAMES: usize = 30;
const BENCH_FRAMES: usize = 240;
const SAMPLE_RATE: u32 = 48_000;
const AUDIO_CHUNK: usize = 480; // samples/channel per injected frame, ~10ms @48kHz

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--check-preset" {
        check_single_preset(Path::new(&args[2]));
        return;
    }

    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out");
    std::fs::create_dir_all(&out_dir).expect("cannot create out/ dir");

    let (egl_inst, display, config) = init_egl();

    // Anchor context (deck 0): every other context below shares its object
    // namespace with this one (EGL_CONTEXT_SHARE_CONTEXT / share_context).
    let ctx0 = create_context(&egl_inst, display, config, None);
    let pb0 = create_pbuffer(&egl_inst, display, config, DECK_W, DECK_H);
    egl_inst.make_current(display, Some(pb0), Some(pb0), Some(ctx0)).unwrap();
    let gl0 = make_gl(&egl_inst);

    // Shared GL objects, created once while ctx0 is current. Textures, buffers
    // and programs ARE shared across the group; VAOs and FBOs are NOT and must
    // be created per-context (each deck's own FBO 0 already exists implicitly
    // via its pbuffer; the compositor gets its own VAO further down).
    let deck_tex: [glow::NativeTexture; DECK_COUNT] = std::array::from_fn(|_| create_shared_deck_texture(&gl0));
    let quad_vbo = create_quad_vbo(&gl0);
    let (composite_program, tex_loc) = create_composite_program(&gl0);

    let mut ctxs = vec![ctx0];
    let mut pbuffers = vec![pb0];
    for _ in 1..DECK_COUNT {
        ctxs.push(create_context(&egl_inst, display, config, Some(ctx0)));
        pbuffers.push(create_pbuffer(&egl_inst, display, config, DECK_W, DECK_H));
    }
    let comp_ctx = create_context(&egl_inst, display, config, Some(ctx0));
    let comp_pb = create_pbuffer(&egl_inst, display, config, COMP_W, COMP_H);

    // One glow::Context per EGL context: function pointers loaded once per context.
    let mut gls: Vec<glow::Context> = vec![gl0];
    for i in 1..DECK_COUNT {
        egl_inst.make_current(display, Some(pbuffers[i]), Some(pbuffers[i]), Some(ctxs[i])).unwrap();
        gls.push(make_gl(&egl_inst));
    }
    egl_inst.make_current(display, Some(comp_pb), Some(comp_pb), Some(comp_ctx)).unwrap();
    let comp_gl = make_gl(&egl_inst);
    let comp_vao = create_comp_vao(&comp_gl, quad_vbo);

    // One projectM instance per deck context, created while that context is current
    // (projectm_create() allocates GL resources immediately: it needs a valid
    // current context, and those resources end up private to that context).
    let preset_dir = preset_dir_arg();
    let first_preset = first_preset_in(&preset_dir);
    println!(
        "[phase0] preset source: {} ({})",
        preset_dir.display(),
        if first_preset.is_some() { "found" } else { "EMPTY, decks will idle on the default preset" }
    );

    let mut pm_handles = Vec::with_capacity(DECK_COUNT);
    for i in 0..DECK_COUNT {
        egl_inst.make_current(display, Some(pbuffers[i]), Some(pbuffers[i]), Some(ctxs[i])).unwrap();
        let handle = unsafe { ffi::projectm_create() };
        assert!(!handle.is_null(), "projectm_create() returned NULL in deck {i} context: GL context not current/valid");
        unsafe {
            ffi::projectm_set_window_size(handle, DECK_W as usize, DECK_H as usize);
            ffi::projectm_set_preset_switch_failed_event_callback(handle, Some(on_preset_failed), std::ptr::null_mut());
            if let Some(p) = &first_preset {
                let c = CString::new(p.to_string_lossy().as_bytes()).unwrap();
                ffi::projectm_load_preset_file(handle, c.as_ptr(), false);
            }
        }
        pm_handles.push(handle);
    }
    println!("[phase0] {DECK_COUNT} deck contexts + 1 compositor context created, projectM instantiated in each deck context.");

    // --- Render loop: proves render-in-own-FBO0, GL state save/restore around
    // render_frame, and a deck->shared-texture copy path that is exclusively
    // glCopyTexSubImage2D (GPU->GPU, no glReadPixels in this path at all: the
    // only glReadPixels in the whole program is the PNG dump below, which is
    // verification tooling, not part of the pipeline being validated).
    let mut render_ms = [0f64; DECK_COUNT];
    let mut copy_ms = [0f64; DECK_COUNT];
    let mut composite_ms = 0f64;
    let mut sample_pos: u64 = 0;
    let mut state_diff_logged = false;

    for frame in 0..(WARMUP_FRAMES + BENCH_FRAMES) {
        let timed = frame >= WARMUP_FRAMES;
        for i in 0..DECK_COUNT {
            egl_inst.make_current(display, Some(pbuffers[i]), Some(pbuffers[i]), Some(ctxs[i])).unwrap();
            let gl = &gls[i];

            let pcm = synth_audio_chunk(sample_pos, i);
            unsafe {
                ffi::projectm_pcm_add_float(
                    pm_handles[i],
                    pcm.as_ptr(),
                    (pcm.len() / 2) as u32,
                    ffi::projectm_channels_PROJECTM_STEREO,
                );
            }

            let before = save_gl_state(gl);
            let t0 = Instant::now();
            if std::env::var("PHASE0_DEBUG_SOLID").is_ok() {
                // Plumbing isolation check: bypass projectM, paint each deck's own
                // FBO 0 a distinct flat color, to test copy+composite independent
                // of whatever a given preset happens to render.
                let c = [(1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0), (1.0, 1.0, 0.0)][i];
                unsafe {
                    gl.clear_color(c.0, c.1, c.2, 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }
            } else {
                unsafe { ffi::projectm_opengl_render_frame(pm_handles[i]) };
            }
            if timed {
                render_ms[i] += t0.elapsed().as_secs_f64() * 1000.0;
            }
            let after = save_gl_state(gl);
            if i == 0 && !state_diff_logged {
                log_state_diff(&before, &after);
                state_diff_logged = true;
            }
            restore_gl_state(gl, &before);

            let t1 = Instant::now();
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(deck_tex[i]));
                gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, 0, 0, DECK_W, DECK_H);
            }
            if timed {
                copy_ms[i] += t1.elapsed().as_secs_f64() * 1000.0;
            }
        }
        sample_pos += AUDIO_CHUNK as u64;

        egl_inst.make_current(display, Some(comp_pb), Some(comp_pb), Some(comp_ctx)).unwrap();
        let t2 = Instant::now();
        composite(&comp_gl, composite_program, tex_loc.as_ref(), comp_vao, &deck_tex);
        if timed {
            composite_ms += t2.elapsed().as_secs_f64() * 1000.0;
        }

        if frame == 0 || frame == WARMUP_FRAMES || frame == WARMUP_FRAMES + BENCH_FRAMES - 1 {
            save_composite_png(&comp_gl, &out_dir, frame);
        }
    }

    println!("\n[phase0] timing over {BENCH_FRAMES} frames (after {WARMUP_FRAMES} warmup, {DECK_W}x{DECK_H}/deck):");
    let mut total_frame_ms = composite_ms / BENCH_FRAMES as f64;
    for i in 0..DECK_COUNT {
        let r = render_ms[i] / BENCH_FRAMES as f64;
        let c = copy_ms[i] / BENCH_FRAMES as f64;
        total_frame_ms += r + c;
        println!("  deck {i}: render {r:.3} ms/frame, copy {c:.3} ms/frame");
    }
    println!("  composite: {:.3} ms/frame", composite_ms / BENCH_FRAMES as f64);
    println!(
        "  total (sequential, 4 decks + composite): {total_frame_ms:.3} ms/frame ({:.1} fps ceiling on this iGPU)",
        1000.0 / total_frame_ms
    );

    // --- Context-switch overhead, isolated from rendering.
    let all_ctx = [ctxs[0], ctxs[1], ctxs[2], ctxs[3], comp_ctx];
    let all_pb = [pbuffers[0], pbuffers[1], pbuffers[2], pbuffers[3], comp_pb];
    const SWITCHES: usize = 2000;
    let t3 = Instant::now();
    for k in 0..SWITCHES {
        let i = k % all_ctx.len();
        egl_inst.make_current(display, Some(all_pb[i]), Some(all_pb[i]), Some(all_ctx[i])).unwrap();
    }
    let switch_ms = t3.elapsed().as_secs_f64() * 1000.0 / SWITCHES as f64;
    println!("  eglMakeCurrent switch: {switch_ms:.4} ms/switch ({:.3} ms for the x4 deck switches per frame)", switch_ms * 4.0);

    // --- Preset compatibility sweep, one subprocess per preset: a genuinely
    // broken preset can segfault the process (confirmed below), not just raise
    // the catchable std::exception projectM normally reports through the
    // failure callback; isolation is required for an honest compat count.
    run_compat_sweep(&preset_dir);

    for h in pm_handles {
        unsafe { ffi::projectm_destroy(h) };
    }
    println!("\n[phase0] done. PNG frames in {}", out_dir.display());
}

// ---------------------------------------------------------------- EGL setup

fn init_egl() -> (Egl, egl::Display, egl::Config) {
    let inst = unsafe { egl::DynamicInstance::<egl::Latest>::load_required() }.expect("failed to load libEGL.so.1");
    let display = unsafe { inst.get_display(egl::DEFAULT_DISPLAY) }.expect("eglGetDisplay failed");
    let (major, minor) = inst.initialize(display).expect("eglInitialize failed");
    println!("[phase0] EGL {major}.{minor} initialized");
    inst.bind_api(egl::OPENGL_API).expect("eglBindAPI(OPENGL_API) failed");

    let config_attribs = [
        egl::SURFACE_TYPE,
        egl::PBUFFER_BIT,
        egl::RENDERABLE_TYPE,
        egl::OPENGL_BIT,
        egl::RED_SIZE,
        8,
        egl::GREEN_SIZE,
        8,
        egl::BLUE_SIZE,
        8,
        egl::ALPHA_SIZE,
        8,
        egl::NONE,
    ];
    let config = inst
        .choose_first_config(display, &config_attribs)
        .expect("eglChooseConfig failed")
        .expect("no matching EGL config for pbuffer+OpenGL");
    (inst, display, config)
}

fn create_context(inst: &Egl, display: egl::Display, config: egl::Config, share: Option<egl::Context>) -> egl::Context {
    let attribs = [
        egl::CONTEXT_MAJOR_VERSION,
        3,
        egl::CONTEXT_MINOR_VERSION,
        3,
        egl::CONTEXT_OPENGL_PROFILE_MASK,
        egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
        egl::NONE,
    ];
    inst.create_context(display, config, share, &attribs).expect("eglCreateContext failed")
}

fn create_pbuffer(inst: &Egl, display: egl::Display, config: egl::Config, w: i32, h: i32) -> egl::Surface {
    let attribs = [egl::WIDTH, w, egl::HEIGHT, h, egl::NONE];
    inst.create_pbuffer_surface(display, config, &attribs).expect("eglCreatePbufferSurface failed")
}

fn make_gl(inst: &Egl) -> glow::Context {
    unsafe { glow::Context::from_loader_function(|s| inst.get_proc_address(s).map_or(std::ptr::null(), |f| f as *const _)) }
}

// --------------------------------------------------------- shared GL objects

fn create_shared_deck_texture(gl: &glow::Context) -> glow::NativeTexture {
    unsafe {
        let tex = gl.create_texture().expect("glGenTextures failed");
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, DECK_W, DECK_H, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        tex
    }
}

fn f32_as_bytes(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

fn create_quad_vbo(gl: &glow::Context) -> glow::NativeBuffer {
    #[rustfmt::skip]
    let verts: [f32; 24] = [
        -1.0, -1.0, 0.0, 0.0,
         1.0, -1.0, 1.0, 0.0,
         1.0,  1.0, 1.0, 1.0,
        -1.0, -1.0, 0.0, 0.0,
         1.0,  1.0, 1.0, 1.0,
        -1.0,  1.0, 0.0, 1.0,
    ];
    unsafe {
        let vbo = gl.create_buffer().expect("glGenBuffers failed");
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, f32_as_bytes(&verts), glow::STATIC_DRAW);
        vbo
    }
}

fn create_comp_vao(gl: &glow::Context, vbo: glow::NativeBuffer) -> glow::NativeVertexArray {
    // VAOs are NOT shared across an EGL share group: this one is local to the
    // compositor context, even though the VBO it points at is a shared object.
    unsafe {
        let vao = gl.create_vertex_array().expect("glGenVertexArrays failed");
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);
        gl.enable_vertex_attrib_array(1);
        vao
    }
}

fn compile_shader(gl: &glow::Context, kind: u32, src: &str) -> glow::NativeShader {
    unsafe {
        let s = gl.create_shader(kind).expect("glCreateShader failed");
        gl.shader_source(s, src);
        gl.compile_shader(s);
        assert!(gl.get_shader_compile_status(s), "shader compile failed: {}", gl.get_shader_info_log(s));
        s
    }
}

fn create_composite_program(gl: &glow::Context) -> (glow::NativeProgram, Option<glow::NativeUniformLocation>) {
    const VS: &str = r#"#version 330 core
layout(location=0) in vec2 in_pos;
layout(location=1) in vec2 in_uv;
out vec2 uv;
void main() {
    uv = in_uv;
    gl_Position = vec4(in_pos, 0.0, 1.0);
}
"#;
    const FS: &str = r#"#version 330 core
in vec2 uv;
out vec4 frag_color;
uniform sampler2D tex;
void main() {
    frag_color = texture(tex, uv);
}
"#;
    unsafe {
        let vs = compile_shader(gl, glow::VERTEX_SHADER, VS);
        let fs = compile_shader(gl, glow::FRAGMENT_SHADER, FS);
        let program = gl.create_program().expect("glCreateProgram failed");
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        assert!(gl.get_program_link_status(program), "compositor shader link failed: {}", gl.get_program_info_log(program));
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        let loc = gl.get_uniform_location(program, "tex");
        (program, loc)
    }
}

fn composite(
    gl: &glow::Context,
    program: glow::NativeProgram,
    tex_loc: Option<&glow::NativeUniformLocation>,
    vao: glow::NativeVertexArray,
    deck_tex: &[glow::NativeTexture; DECK_COUNT],
) {
    unsafe {
        gl.viewport(0, 0, COMP_W, COMP_H);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.use_program(Some(program));
        gl.bind_vertex_array(Some(vao));
        gl.active_texture(glow::TEXTURE0);
        gl.uniform_1_i32(tex_loc, 0);
        // deck0=top-left, deck1=top-right, deck2=bottom-left, deck3=bottom-right.
        let quadrants = [(0, DECK_H), (DECK_W, DECK_H), (0, 0), (DECK_W, 0)];
        for (i, (x, y)) in quadrants.iter().enumerate() {
            gl.viewport(*x, *y, DECK_W, DECK_H);
            gl.bind_texture(glow::TEXTURE_2D, Some(deck_tex[i]));
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
        }
    }
}

// ------------------------------------------------------- GL state save/restore
// Defends against projectM leaking GL state to the caller (upstream PR #981,
// still open as of this writing): save before render_frame, restore after.

struct GlState {
    program: i32,
    vao: i32,
    blend_enabled: bool,
    blend_src_rgb: i32,
    blend_dst_rgb: i32,
    blend_src_alpha: i32,
    blend_dst_alpha: i32,
    blend_eq_rgb: i32,
    blend_eq_alpha: i32,
    active_texture: i32,
}

fn save_gl_state(gl: &glow::Context) -> GlState {
    unsafe {
        GlState {
            program: gl.get_parameter_i32(glow::CURRENT_PROGRAM),
            vao: gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING),
            blend_enabled: gl.is_enabled(glow::BLEND),
            blend_src_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
            blend_dst_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
            blend_src_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
            blend_dst_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
            blend_eq_rgb: gl.get_parameter_i32(glow::BLEND_EQUATION_RGB),
            blend_eq_alpha: gl.get_parameter_i32(glow::BLEND_EQUATION_ALPHA),
            active_texture: gl.get_parameter_i32(glow::ACTIVE_TEXTURE),
        }
    }
}

fn restore_gl_state(gl: &glow::Context, s: &GlState) {
    unsafe {
        gl.use_program(NonZeroU32::new(s.program as u32).map(glow::NativeProgram));
        gl.bind_vertex_array(NonZeroU32::new(s.vao as u32).map(glow::NativeVertexArray));
        if s.blend_enabled {
            gl.enable(glow::BLEND);
        } else {
            gl.disable(glow::BLEND);
        }
        gl.blend_func_separate(s.blend_src_rgb as u32, s.blend_dst_rgb as u32, s.blend_src_alpha as u32, s.blend_dst_alpha as u32);
        gl.blend_equation_separate(s.blend_eq_rgb as u32, s.blend_eq_alpha as u32);
        gl.active_texture(s.active_texture as u32);
    }
}

fn log_state_diff(before: &GlState, after: &GlState) {
    let changed = before.program != after.program
        || before.vao != after.vao
        || before.blend_enabled != after.blend_enabled
        || before.blend_src_rgb != after.blend_src_rgb
        || before.blend_dst_rgb != after.blend_dst_rgb;
    println!(
        "[phase0] projectm_opengl_render_frame GL state check (deck 0, frame 0): {}",
        if changed { "state DID change (confirms PR #981 concern: restore is load-bearing)" } else { "state unchanged this run" }
    );
}

// -------------------------------------------------------------- audio (fake)

fn synth_audio_chunk(sample_pos: u64, deck: usize) -> Vec<f32> {
    let mut buf = Vec::with_capacity(AUDIO_CHUNK * 2);
    let base_freq = 220.0 + deck as f32 * 55.0;
    for n in 0..AUDIO_CHUNK {
        let t = (sample_pos + n as u64) as f32 / SAMPLE_RATE as f32;
        let tone = (t * base_freq * std::f32::consts::TAU).sin() * 0.3;
        let beat_phase = (t * 2.0) % 1.0; // ~2 Hz synthetic kick
        let kick = if beat_phase < 0.02 { (1.0 - beat_phase / 0.02) * 0.6 } else { 0.0 };
        let s = (tone + kick).clamp(-1.0, 1.0);
        buf.push(s);
        buf.push(s);
    }
    buf
}

// ------------------------------------------------------------------ PNG dump

fn save_composite_png(gl: &glow::Context, out_dir: &Path, frame: usize) {
    let mut pixels = vec![0u8; (COMP_W * COMP_H * 4) as usize];
    unsafe {
        gl.read_pixels(0, 0, COMP_W, COMP_H, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut pixels));
    }
    // GL row 0 is the bottom row; flip for a conventional top-down PNG.
    let row_bytes = (COMP_W * 4) as usize;
    let mut flipped = vec![0u8; pixels.len()];
    for y in 0..COMP_H as usize {
        let dst_y = COMP_H as usize - 1 - y;
        flipped[dst_y * row_bytes..(dst_y + 1) * row_bytes].copy_from_slice(&pixels[y * row_bytes..(y + 1) * row_bytes]);
    }
    let path = out_dir.join(format!("composite_frame_{frame:04}.png"));
    image::save_buffer(&path, &flipped, COMP_W as u32, COMP_H as u32, image::ColorType::Rgba8).expect("failed to write PNG");
    println!("[phase0] wrote {}", path.display());
}

// --------------------------------------------------------------- presets

fn preset_dir_arg() -> PathBuf {
    let raw = std::env::var("PHASE0_PRESET_DIR").unwrap_or_else(|_| {
        panic!(
            "PHASE0_PRESET_DIR is not set. Point it at a directory of .milk presets, e.g.:\n  \
             PHASE0_PRESET_DIR=/path/to/presets cargo run"
        )
    });
    PathBuf::from(raw)
}

fn walk_milk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk_milk_files(&p));
            } else if p.extension().map(|e| e.eq_ignore_ascii_case("milk")).unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out
}

fn first_preset_in(dir: &Path) -> Option<PathBuf> {
    let mut entries = walk_milk_files(dir);
    entries.sort();
    // Skip transition-only presets (e.g. "! Transition/...to black...") for the
    // decks that render continuously: they're legitimately near-static/black,
    // not a bug, but a bad default for eyeballing that rendering works.
    entries
        .iter()
        .find(|p| !p.to_string_lossy().contains("Transition"))
        .cloned()
        .or_else(|| entries.into_iter().next())
}

static PRESET_FAILED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn on_preset_failed(_filename: *const c_char, message: *const c_char, _user_data: *mut c_void) {
    PRESET_FAILED.store(true, Ordering::SeqCst);
    let msg = unsafe { CStr::from_ptr(message) }.to_string_lossy();
    eprintln!("[phase0]   preset failed: {msg}");
}

/// Runs entirely inside a `--check-preset` child process: load one preset,
/// render a few frames, exit 1 if projectM's failure callback fired. A crash
/// (segfault, abort) shows up to the parent as death-by-signal, not exit(1).
fn check_single_preset(path: &Path) {
    let (egl_inst, display, config) = init_egl();
    let ctx = create_context(&egl_inst, display, config, None);
    let pb = create_pbuffer(&egl_inst, display, config, DECK_W, DECK_H);
    egl_inst.make_current(display, Some(pb), Some(pb), Some(ctx)).unwrap();

    let handle = unsafe { ffi::projectm_create() };
    assert!(!handle.is_null(), "projectm_create() returned NULL");
    unsafe {
        ffi::projectm_set_window_size(handle, DECK_W as usize, DECK_H as usize);
        ffi::projectm_set_preset_switch_failed_event_callback(handle, Some(on_preset_failed), std::ptr::null_mut());
        let c = CString::new(path.to_string_lossy().as_bytes()).unwrap();
        ffi::projectm_load_preset_file(handle, c.as_ptr(), false);
        for _ in 0..5 {
            ffi::projectm_opengl_render_frame(handle);
        }
    }
    std::process::exit(if PRESET_FAILED.load(Ordering::SeqCst) { 1 } else { 0 });
}

fn run_compat_sweep(preset_dir: &Path) {
    let mut all = walk_milk_files(preset_dir);
    all.sort();
    const SAMPLE: usize = 200;
    if all.is_empty() {
        println!("\n[phase0] preset compat sweep: no .milk files found under {}", preset_dir.display());
        return;
    }
    let step = (all.len() / SAMPLE).max(1);
    let sample: Vec<&PathBuf> = all.iter().step_by(step).take(SAMPLE).collect();
    let exe = std::env::current_exe().expect("current_exe() failed");

    let (mut pass, mut reported_fail, mut crashed) = (0usize, 0usize, 0usize);
    for p in &sample {
        let status = std::process::Command::new(&exe)
            .arg("--check-preset")
            .arg(p)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        use std::os::unix::process::ExitStatusExt;
        match status {
            Ok(s) if s.success() => pass += 1,
            Ok(s) if s.signal().is_some() => crashed += 1,
            Ok(_) => reported_fail += 1,
            Err(e) => {
                eprintln!("[phase0] failed to spawn preset checker: {e}");
                crashed += 1;
            }
        }
    }
    println!(
        "\n[phase0] preset compat sweep: {} candidates, {} sampled, subprocess-isolated:\n  \
         {pass} pass, {reported_fail} reported failure (caught), {crashed} crashed (killed by signal), {:.1}% usable",
        all.len(),
        sample.len(),
        100.0 * pass as f64 / sample.len().max(1) as f64
    );
}
