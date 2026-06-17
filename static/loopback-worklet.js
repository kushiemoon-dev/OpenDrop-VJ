/**
 * loopback-worklet.js — AudioWorkletProcessor for per-device loopback capture.
 *
 * Receives Int16 PCM chunks from the Electron main process (via IPC + port.postMessage),
 * stores them in a ring buffer, and outputs Float32 samples to the Web Audio graph.
 * Performs linear interpolation resampling when srcRate ≠ AudioContext.sampleRate.
 *
 * Loaded via ctx.audioWorklet.addModule('/loopback-worklet.js').
 * Registered as 'loopback-pcm'.
 */

class LoopbackPcmProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    // Ring buffer — must be power of 2. 65536 samples ≈ 1.5 s at 44.1 kHz.
    this._SIZE = 65536;
    this._ringL = new Float32Array(this._SIZE);
    this._ringR = new Float32Array(this._SIZE);
    // writePos: integer, counts total samples written (ever-increasing).
    // readPos:  fractional, counts total samples consumed (includes resampling ratio).
    this._writePos = 0;
    this._readPos = 0.0;

    this._srcRate = 44100;
    this._channels = 2;

    this.port.onmessage = (e) => {
      const { sampleRate, channels, pcm } = e.data; // pcm: Int16Array (transferred)
      this._srcRate = sampleRate;
      this._channels = channels;

      const frames = pcm.length / channels;
      const MASK = this._SIZE - 1;

      for (let i = 0; i < frames; i++) {
        const l = pcm[i * channels] / 32768;
        const r = channels > 1 ? pcm[i * channels + 1] / 32768 : l;
        this._ringL[this._writePos & MASK] = l;
        this._ringR[this._writePos & MASK] = r;
        this._writePos++;
      }

      // Overrun guard: if read falls more than SIZE/2 behind write, jump forward.
      if (this._writePos - Math.floor(this._readPos) > this._SIZE / 2) {
        this._readPos = this._writePos - 4096; // keep ~93 ms buffer at 44.1 kHz
      }
    };
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    if (!out || !out[0]) return true;

    const outL = out[0];
    const outR = out[1] ?? out[0];
    const n = outL.length; // typically 128 frames
    // sampleRate is the AudioWorkletGlobalScope global (= AudioContext.sampleRate).
    const ratio = this._srcRate / sampleRate;
    const MASK = this._SIZE - 1;

    for (let i = 0; i < n; i++) {
      const available = this._writePos - Math.floor(this._readPos);
      if (available < 2) {
        // Underrun — output silence rather than garbage.
        outL[i] = 0;
        outR[i] = 0;
      } else {
        const ipos = Math.floor(this._readPos);
        const frac = this._readPos - ipos;
        const i0 = ipos & MASK;
        const i1 = (ipos + 1) & MASK;
        outL[i] = this._ringL[i0] * (1 - frac) + this._ringL[i1] * frac;
        outR[i] = this._ringR[i0] * (1 - frac) + this._ringR[i1] * frac;
        this._readPos += ratio;
      }
    }

    return true; // keep processor alive indefinitely
  }
}

registerProcessor('loopback-pcm', LoopbackPcmProcessor);
