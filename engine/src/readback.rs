//! Async double-buffered PBO readback of a shared GL texture's pixels.
//!
//! Structural mirror of `engine::timing::PassTimer`: a small ring of GPU
//! resources, a `write_idx` that advances one slot per frame, and a
//! non-blocking `poll()` that only returns data once the GPU side has
//! actually finished: never by waiting for it. Here the ring holds 2
//! Pixel Buffer Objects instead of `GL_TIME_ELAPSED` queries, and what's
//! polled is a `glReadPixels` DMA into a PBO instead of a timer result.
//!
//! `glReadPixels` while a buffer is bound to `GL_PIXEL_PACK_BUFFER` only
//! *queues* the GPU->CPU transfer: it returns immediately, unlike the
//! no-PBO form which blocks until the copy lands. That's what makes
//! `begin_read` cheap. The catch is on the read side: `glMapBufferRange`
//! must know the transfer is actually done before handing back a pointer,
//! or the caller reads garbage. The obvious-looking fix,
//! `GL_MAP_UNSYNCHRONIZED_BIT`, is a dead end for this: the GL 4
//! spec (glMapBufferRange, error list) makes `GL_MAP_READ_BIT` combined
//! with `GL_MAP_UNSYNCHRONIZED_BIT` an explicit `GL_INVALID_OPERATION`
//! (NULL return); that flag is a write-side streaming tool only. So
//! completion here is tracked the same way `PassTimer` tracks its
//! queries: a real non-blocking availability check: just with a fence
//! sync object standing in for `GL_QUERY_RESULT_AVAILABLE`:
//! `glFenceSync` right after `glReadPixels`, then `glClientWaitSync` with
//! a 0ns timeout, which by spec returns immediately
//! (`ALREADY_SIGNALED`/`CONDITION_SATISFIED` vs `TIMEOUT_EXPIRED`) rather
//! than blocking. Only once that fence has signaled do we call
//! `map_buffer_range` with plain `GL_MAP_READ_BIT`: at that point the
//! transfer is already known complete, so the map itself has nothing left
//! to wait for either.

use glow::HasContext;

const RING_LEN: usize = 2;

pub struct FrameReadback {
    /// Attaches `texture` (passed to `new`) as its sole color attachment,
    /// once, at construction: never reattached afterward.
    read_fbo: glow::NativeFramebuffer,
    pbos: [glow::NativeBuffer; RING_LEN],
    /// One fence per slot: `Some` while that slot's `glReadPixels` DMA is
    /// still outstanding or unpolled, cleared to `None` once `poll` has
    /// consumed it (or, if a slot is reused before ever being polled, when
    /// `begin_read` retires it early to avoid leaking the sync object).
    fences: [Option<glow::NativeFence>; RING_LEN],
    write_idx: usize,
    w: u32,
    h: u32,
}

impl FrameReadback {
    /// Must run on the main context: it's the only one that sees every
    /// texture in the share group. `texture` may be `compositor.color_tex`
    /// or any `deck.texture`; both live in the same share group, so one
    /// `FrameReadback` per texture serves either case identically.
    pub fn new(gl: &glow::Context, texture: glow::NativeTexture, w: u32, h: u32) -> Result<Self, String> {
        unsafe {
            let read_fbo = gl.create_framebuffer().map_err(|e| format!("create_framebuffer (readback) failed: {e}"))?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(read_fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                return Err(format!("readback FBO incomplete: status 0x{status:x}"));
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            let size = (w * h * 4) as i32; // RGBA8, native format: no BGRA swizzle
            let mut pbos = Vec::with_capacity(RING_LEN);
            for i in 0..RING_LEN {
                let pbo = gl.create_buffer().map_err(|e| format!("create_buffer (pbo{i}) failed: {e}"))?;
                gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(pbo));
                gl.buffer_data_size(glow::PIXEL_PACK_BUFFER, size, glow::STREAM_READ);
                pbos.push(pbo);
            }
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);

            Ok(Self {
                read_fbo,
                pbos: pbos.try_into().expect("RING_LEN buffers pushed"),
                fences: [None; RING_LEN],
                write_idx: 0,
                w,
                h,
            })
        }
    }

    /// Issues this frame's `glReadPixels` into the current write slot's PBO
    /// and advances to the other slot for next call. Never synchronizes,
    /// never stalls: with a PBO bound to `GL_PIXEL_PACK_BUFFER`, the copy
    /// is only queued, not waited on. Call once per frame, main context
    /// current.
    pub fn begin_read(&mut self, gl: &glow::Context) {
        let slot = self.write_idx;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.read_fbo));
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(self.pbos[slot]));
            gl.read_pixels(0, 0, self.w as i32, self.h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::BufferOffset(0));
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            // If this slot's previous fence was never consumed by `poll`
            // (a frame got dropped), delete it now: otherwise it leaks,
            // since a fresh one is about to replace it below.
            if let Some(old) = self.fences[slot].take() {
                gl.delete_sync(old);
            }
            self.fences[slot] = Some(gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0).expect("fence_sync failed"));
        }
        self.write_idx = (self.write_idx + 1) % RING_LEN;
    }

    /// Non-blocking: returns the RGBA8 bytes from whichever slot's
    /// `glReadPixels` has actually landed (checked via a 0ns
    /// `glClientWaitSync`, which by spec returns immediately either way:
    /// see the module doc for why that replaces `GL_MAP_UNSYNCHRONIZED_BIT`
    /// here), or `None` if neither slot is ready yet. Mirrors
    /// `PassTimer::poll` (`engine/src/timing.rs:61-68`): same
    /// check-before-you-touch-it idiom, pixel bytes instead of a time
    /// value.
    pub fn poll(&mut self, gl: &glow::Context) -> Option<Vec<u8>> {
        for slot in 0..RING_LEN {
            let Some(fence) = self.fences[slot] else { continue };
            let status = unsafe { gl.client_wait_sync(fence, glow::SYNC_FLUSH_COMMANDS_BIT, 0) };
            if status != glow::ALREADY_SIGNALED && status != glow::CONDITION_SATISFIED {
                continue;
            }

            let size = (self.w * self.h * 4) as usize;
            let data = unsafe {
                gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(self.pbos[slot]));
                let ptr = gl.map_buffer_range(glow::PIXEL_PACK_BUFFER, 0, size as i32, glow::MAP_READ_BIT);
                if ptr.is_null() {
                    gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
                    continue;
                }
                let data = std::slice::from_raw_parts(ptr, size).to_vec();
                gl.unmap_buffer(glow::PIXEL_PACK_BUFFER);
                gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
                gl.delete_sync(fence);
                data
            };
            self.fences[slot] = None;
            return Some(data);
        }
        None
    }
}
