/**
 * capture-worklet.js — AudioWorkletProcessor that captures the live audio signal
 * and posts Int16 PCM chunks to the main thread for relay to the output window.
 *
 * Accumulates Float32 input samples into 1920-frame interleaved Int16 chunks
 * (~40 ms at 48 kHz) and posts { sampleRate, channels, pcm: Int16Array } messages.
 * The format is identical to what loopback-worklet.js (loopback-pcm) expects.
 *
 * Loaded via ctx.audioWorklet.addModule('/capture-worklet.js').
 * Registered as 'capture-pcm'.
 */

class CapturePcmProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this._CHUNK = 1920; // frames per message — ~40 ms at 48 kHz
    this._bufL = new Float32Array(this._CHUNK);
    this._bufR = new Float32Array(this._CHUNK);
    this._fill = 0;
  }

  process(inputs) {
    const input = inputs[0];
    // Keep processor alive even when no input is connected yet.
    if (!input || input.length === 0) return true;

    const inL = input[0];
    const inR = input[1] ?? input[0]; // mono source → duplicate to right channel
    const n = inL.length; // typically 128 frames per quantum

    for (let i = 0; i < n; i++) {
      this._bufL[this._fill] = inL[i];
      this._bufR[this._fill] = inR[i];
      this._fill++;

      if (this._fill === this._CHUNK) {
        const pcm = new Int16Array(this._CHUNK * 2); // interleaved L/R
        for (let f = 0; f < this._CHUNK; f++) {
          // Clamp then convert Float32 → Int16.
          // Use ×32768 for negative, ×32767 for positive — matches the /32768
          // division in loopback-worklet.js which keeps values in [-1, ~1).
          let l = this._bufL[f];
          let r = this._bufR[f];
          l = l < -1 ? -1 : l > 1 ? 1 : l;
          r = r < -1 ? -1 : r > 1 ? 1 : r;
          pcm[f * 2]     = l < 0 ? (l * 32768) | 0 : (l * 32767) | 0;
          pcm[f * 2 + 1] = r < 0 ? (r * 32768) | 0 : (r * 32767) | 0;
        }
        // Transfer the buffer to avoid a copy on the intra-process boundary.
        // sampleRate is the AudioWorkletGlobalScope global (= AudioContext.sampleRate).
        this.port.postMessage({ sampleRate, channels: 2, pcm }, [pcm.buffer]);
        this._fill = 0;
      }
    }

    return true; // keep processor alive indefinitely
  }
}

registerProcessor('capture-pcm', CapturePcmProcessor);
