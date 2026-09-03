//! Per-context `GL_TIME_ELAPSED` query pools.
//!
//! A query's result usually isn't ready the instant you ask: GPU work is
//! asynchronous, so blocking on `glGetQueryObjectuiv(..., GL_QUERY_RESULT)`
//! right after `glEndQuery` would stall the pipeline waiting for the very
//! frame you just submitted. `PassTimer` avoids that by keeping a ring of 3
//! queries per pass: `begin()` starts this frame's query in the next ring
//! slot, and, before doing that, non-blockingly polls whatever query
//! previously occupied that slot (from up to `RING_LEN` frames ago, which
//! on any real driver has long since resolved) via
//! `GL_QUERY_RESULT_AVAILABLE`. That poll happens inline, in whichever
//! context is already current for the pass's own `begin()`/`end()` calls:
//! never a separate `make_current` just to read a query back.
//!
//! `GL_TIME_ELAPSED` queries cannot nest within one context: a deck's
//! render and copy passes are sequential (`render.end()` before
//! `copy.begin()`), never overlapping.
//!
//! What this measures is GPU execution time for one pass in one context:
//! it does NOT sum into "frame time". Passes across different contexts can
//! overlap on real hardware, and even within one context, submission and
//! execution aren't the same clock. Wall-clock swap-to-swap time is the
//! only true "frame time"; report it separately, never as a sum of these.

use glow::HasContext;

const RING_LEN: usize = 3;

pub struct PassTimer {
    queries: Vec<glow::NativeQuery>,
    write_idx: usize,
    last_ms: Option<f64>,
}

impl PassTimer {
    /// Must run while the owning context is current: queries, like FBOs
    /// and VAOs, are not shared across the GL share group.
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let mut queries = Vec::with_capacity(RING_LEN);
        for _ in 0..RING_LEN {
            queries.push(unsafe { gl.create_query() }.map_err(|e| format!("create_query failed: {e}"))?);
        }
        Ok(Self { queries, write_idx: 0, last_ms: None })
    }

    /// Starts this frame's query. Must run while the owning context is
    /// current; must not be nested with another `PassTimer::begin` on the
    /// same context (GL only allows one active `GL_TIME_ELAPSED` query per
    /// context: `end()` the previous pass first).
    pub fn begin(&mut self, gl: &glow::Context) {
        self.poll(gl);
        unsafe { gl.begin_query(glow::TIME_ELAPSED, self.queries[self.write_idx]) };
    }

    pub fn end(&mut self, gl: &glow::Context) {
        unsafe { gl.end_query(glow::TIME_ELAPSED) };
        self.write_idx = (self.write_idx + 1) % RING_LEN;
    }

    /// Non-blocking readback of whichever query is about to be reused.
    ///
    /// **Known pre-existing bug, deliberately not fixed here** (found during
    /// Phase 8 Step 10's review, out of scope for that task and for the
    /// Phase 8 fix wave): on the first `RING_LEN` frames this queries a
    /// `NativeQuery` that `create_query` allocated but no `glBeginQuery` has
    /// ever targeted, so `GL_QUERY_RESULT_AVAILABLE` raises
    /// `GL_INVALID_OPERATION` (a name is only a query *object* once first
    /// bound). Harmless: the readback is skipped, `last_ms` stays `None`,
    /// and it self-corrects once `write_idx` has wrapped, but it does dirty
    /// the GL error queue at startup, which can mislead anything else
    /// calling `glGetError`. Fix would be to track how many slots have been
    /// written and skip `poll` until the ring is full.
    fn poll(&mut self, gl: &glow::Context) {
        let q = self.queries[self.write_idx];
        let available = unsafe { gl.get_query_parameter_u32(q, glow::QUERY_RESULT_AVAILABLE) } != 0;
        if available {
            let ns = unsafe { gl.get_query_parameter_u64(q, glow::QUERY_RESULT) };
            self.last_ms = Some(ns as f64 / 1_000_000.0);
        }
    }

    /// Milliseconds from the most recent query that had resolved by the
    /// time its slot was polled. `None` until the first `RING_LEN` frames
    /// have gone by.
    pub fn last_ms(&self) -> Option<f64> {
        self.last_ms
    }
}
