'use strict';

const { app, BrowserWindow, ipcMain, desktopCapturer, protocol, session } = require('electron');
const path = require('path');
const fs = require('fs');
const { spawn } = require('child_process');

// Allow AudioContext to auto-start in renderer windows (including the output window
// which opens programmatically via window.open, with no user gesture of its own).
// Must be set before app is ready.
app.commandLine.appendSwitch('autoplay-policy', 'no-user-gesture-required');

// Must be called before app is ready — makes app:// behave like https://
// (standard origin, secure context, fetch API, dynamic import support)
protocol.registerSchemesAsPrivileged([{
  scheme: 'app',
  privileges: { standard: true, secure: true, supportFetchAPI: true, stream: true },
}]);

const MIME = {
  '.html': 'text/html',
  '.js':   'text/javascript',
  '.mjs':  'text/javascript',
  '.css':  'text/css',
  '.json': 'application/json',
  '.svg':  'image/svg+xml',
  '.png':  'image/png',
  '.jpg':  'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.woff2':'font/woff2',
  '.woff': 'font/woff',
  '.ttf':  'font/ttf',
  '.ico':  'image/x-icon',
  '.mp4':  'video/mp4',
  '.webm': 'video/webm',
  '.mov':  'video/quicktime',
  '.m4v':  'video/x-m4v',
};

// ── NDI (optional — requires NDI SDK + grandiose) ──────────────────────────
let grandiose = null;
let ndiSender = null;
let ndiTimer = null;
let outputWin = null;     // tracked below in did-create-window

// ── Spout (optional — Windows only, requires spout-addon + SpoutDX vendor sources) ──
let spout = null;
let spoutTimer = null;
try { spout = require('spout-addon'); } catch { /* non-Windows or addon not built */ }

// ── Per-device output loopback (optional — Windows, requires audify) ──
let loopbackRt = null;

// ── v4l2loopback (Linux — via ffmpeg pipe, no native module required) ──────
let v4l2Proc = null;
let v4l2Timer = null;
let v4l2Draining = false;
let v4l2W = 0;
let v4l2H = 0;
let v4l2Error = '';

/** Find the first v4l2loopback device whose label contains "OpenDrop".
 *  Reads /sys/class/video4linux/video*/name — pure fs, no exec.
 *  Returns "/dev/videoN" or null.
 */
function findV4l2Device() {
  try {
    const base = '/sys/class/video4linux';
    const entries = fs.readdirSync(base);
    for (const entry of entries) {
      const namePath = path.join(base, entry, 'name');
      try {
        const label = fs.readFileSync(namePath, 'utf8').trim();
        if (label.includes('OpenDrop')) return `/dev/${entry}`;
      } catch { /* entry has no name file — skip */ }
    }
  } catch { /* /sys not available (non-Linux) */ }
  return null;
}

const isDev = !app.isPackaged;
const DEV_URL = 'http://localhost:1420';
const BUILD_DIR = path.join(__dirname, '../build');

// ── Relay BroadcastChannel messages between renderer processes ─────────────
ipcMain.on('bc-post', (event, data) => {
  BrowserWindow.getAllWindows().forEach((win) => {
    if (win.webContents.id !== event.sender.id) {
      win.webContents.send('bc-msg', data);
    }
  });
});

// ── Relay raw PCM audio frames from main renderer to output window ──────────
// Dedicated channel (not bc-post) to keep ~190 KB/s PCM traffic separate from
// low-rate control messages (preset/crossfader/beat/overlays/video).
ipcMain.on('audioframe:post', (event, data) => {
  BrowserWindow.getAllWindows().forEach((win) => {
    if (win.webContents.id !== event.sender.id) {
      win.webContents.send('audioframe:data', data);
    }
  });
});

// ── Platform info ───────────────────────────────────────────────────────────
ipcMain.handle('get-platform', () => process.platform);

// ── NDI handlers ─────────────────────────────────────────────────────────────
ipcMain.handle('ndi:start', async (_, { name, width, height }) => {
  try {
    if (!grandiose) grandiose = require('grandiose');
    if (ndiSender) { ndiSender.destroy?.(); ndiSender = null; }
    if (ndiTimer) { clearInterval(ndiTimer); ndiTimer = null; }

    ndiSender = await grandiose.send({ name, clockVideo: false, clockAudio: false });

    ndiTimer = setInterval(async () => {
      const win = outputWin && !outputWin.isDestroyed() ? outputWin : null;
      if (!win || !ndiSender) return;
      try {
        const img = await win.webContents.capturePage();
        const buf = img.toBitmap();
        const w = img.getSize().width;
        const h = img.getSize().height;
        if (!w || !h) return;
        ndiSender.video({
          xres: w,
          yres: h,
          frameRateN: 30000,
          frameRateD: 1001,
          pictureAspectRatio: w / h,
          type: grandiose.FRAME_TYPE_VIDEO,
          lineStrideBytes: w * 4,
          fourCC: grandiose.FOURCC_BGRA,
          data: buf,
          timestamp: BigInt(0),
        }).catch(() => {});
      } catch { /* skip frame on error */ }
    }, 1000 / 30);

    return { ok: true };
  } catch (e) {
    return { ok: false, error: e.message };
  }
});

ipcMain.handle('ndi:stop', async () => {
  if (ndiTimer) { clearInterval(ndiTimer); ndiTimer = null; }
  ndiSender?.destroy?.();
  ndiSender = null;
  return { ok: true };
});

// ── Spout handlers ────────────────────────────────────────────────────────
ipcMain.handle('spout:start', async (_, { name }) => {
  try {
    if (process.platform !== 'win32') return { ok: false, error: 'Spout est disponible uniquement sur Windows.' };
    if (!spout) return { ok: false, error: 'spout-addon non disponible — recompilez avec : pnpm run electron:rebuild:spout' };
    if (spoutTimer) { clearInterval(spoutTimer); spoutTimer = null; }
    spout.stop();

    const ok = spout.init(name || 'OpenDrop VJ');
    if (!ok) return { ok: false, error: 'Échec OpenDirectX11 — aucun GPU DirectX 11 disponible.' };

    spoutTimer = setInterval(async () => {
      const win = outputWin && !outputWin.isDestroyed() ? outputWin : null;
      if (!win) return;
      try {
        const img = await win.webContents.capturePage();
        const buf = img.toBitmap(); // BGRA top-down, w*4 stride — exact match for SendImage
        const { width: w, height: h } = img.getSize();
        if (!w || !h) return;
        spout.send(buf, w, h);
      } catch { /* skip frame on error */ }
    }, 1000 / 30);

    return { ok: true };
  } catch (e) {
    return { ok: false, error: e.message };
  }
});

ipcMain.handle('spout:stop', () => {
  if (spoutTimer) { clearInterval(spoutTimer); spoutTimer = null; }
  try { spout?.stop(); } catch {}
  return { ok: true };
});

// ── v4l2loopback handlers ─────────────────────────────────────────────────
ipcMain.handle('v4l2:start', async () => {
  // Teardown any prior session
  if (v4l2Timer) { clearInterval(v4l2Timer); v4l2Timer = null; }
  if (v4l2Proc) { try { v4l2Proc.stdin.end(); v4l2Proc.kill('SIGTERM'); } catch {} v4l2Proc = null; }
  v4l2Draining = false;
  v4l2Error = '';

  // Locate device before doing anything else
  const devPath = findV4l2Device();
  if (!devPath) {
    return { ok: false, error: "Aucun device v4l2loopback 'OpenDrop' trouvé. Lance scripts/setup-v4l2.sh puis réessaie." };
  }

  // Capture one frame to lock W×H (rawvideo needs a fixed size at spawn time)
  const win = outputWin && !outputWin.isDestroyed() ? outputWin : null;
  if (!win) return { ok: false, error: 'Fenêtre output introuvable.' };

  let firstImg;
  try {
    firstImg = await win.webContents.capturePage();
  } catch (e) {
    return { ok: false, error: `Capture initiale échouée : ${e.message}` };
  }
  v4l2W = firstImg.getSize().width;
  v4l2H = firstImg.getSize().height;
  if (!v4l2W || !v4l2H) return { ok: false, error: 'Résolution nulle — la fenêtre output est-elle visible ?' };

  // Spawn ffmpeg: read raw BGRA from stdin, emit YUV420p to the v4l2 device
  const proc = spawn('ffmpeg', [
    '-f', 'rawvideo',
    '-pix_fmt', 'bgra',
    '-s', `${v4l2W}x${v4l2H}`,
    '-r', '30',
    '-i', 'pipe:0',
    '-f', 'v4l2',
    '-pix_fmt', 'yuv420p',
    devPath,
  ], { stdio: ['pipe', 'ignore', 'pipe'] });

  proc.on('error', (e) => { v4l2Error = e.code === 'ENOENT' ? 'ffmpeg introuvable — installe ffmpeg et réessaie.' : e.message; });
  proc.stderr.on('data', (chunk) => { v4l2Error = chunk.toString().trim().split('\n').pop() ?? v4l2Error; });
  proc.on('exit', (code) => {
    if (code !== 0 && code !== null) v4l2Error = v4l2Error || `ffmpeg a quitté avec le code ${code}.`;
    if (v4l2Timer) { clearInterval(v4l2Timer); v4l2Timer = null; }
    v4l2Proc = null;
  });
  proc.stdin.on('drain', () => { v4l2Draining = false; });

  v4l2Proc = proc;

  // Allow ffmpeg a short moment to fail fast (ENOENT, bad device, etc.)
  await new Promise((r) => setTimeout(r, 150));
  if (!v4l2Proc || v4l2Error.length > 0) {
    return { ok: false, error: v4l2Error || 'ffmpeg a quitté immédiatement.' };
  }

  // Push first frame right away so the stream starts immediately
  const firstBuf = firstImg.toBitmap();
  if (!v4l2Draining) {
    const ok = v4l2Proc.stdin.write(firstBuf);
    if (!ok) v4l2Draining = true;
  }

  // Capture-and-write loop @30fps
  v4l2Timer = setInterval(async () => {
    const w = outputWin && !outputWin.isDestroyed() ? outputWin : null;
    if (!w || !v4l2Proc || v4l2Draining) return;
    try {
      const img = await w.webContents.capturePage();
      const { width, height } = img.getSize();
      // Drop frames whose size changed — rawvideo can't handle mid-stream resize
      if (width !== v4l2W || height !== v4l2H) return;
      const written = v4l2Proc.stdin.write(img.toBitmap());
      if (!written) v4l2Draining = true;
    } catch { /* skip frame on error */ }
  }, 1000 / 30);

  return { ok: true };
});

ipcMain.handle('v4l2:stop', async () => {
  if (v4l2Timer) { clearInterval(v4l2Timer); v4l2Timer = null; }
  if (v4l2Proc) {
    try { v4l2Proc.stdin.end(); } catch {}
    try { v4l2Proc.kill('SIGTERM'); } catch {}
    v4l2Proc = null;
  }
  v4l2W = 0;
  v4l2H = 0;
  v4l2Draining = false;
  return { ok: true };
});

// ── Per-device output loopback handlers ────────────────────────────────────
ipcMain.handle('loopback:list', async () => {
  try {
    const { RtAudio, RtAudioApi } = require('audify');
    const rt = new RtAudio(process.platform === 'win32' ? RtAudioApi.WINDOWS_WASAPI : undefined);
    const devices = rt.getDevices();
    // Output devices with outputChannels > 0 can be captured via WASAPI loopback
    // by opening them as input streams — RtAudio applies AUDCLNT_STREAMFLAGS_LOOPBACK.
    const outputs = devices
      .filter((d) => d.outputChannels > 0)
      .map((d) => ({
        id: d.id,
        name: d.name,
        maxInputChannels: d.inputChannels,
        maxOutputChannels: d.outputChannels,
        defaultSampleRate: d.preferredSampleRate,
      }));
    return { ok: true, devices: outputs };
  } catch (e) {
    return { ok: false, error: e.message, devices: [] };
  }
});

ipcMain.handle('loopback:start', async (_, { deviceId }) => {
  try {
    const { RtAudio, RtAudioApi, RtAudioFormat } = require('audify');
    // Idempotent teardown
    if (loopbackRt) { try { loopbackRt.closeStream(); } catch {} loopbackRt = null; }

    const rt = new RtAudio(process.platform === 'win32' ? RtAudioApi.WINDOWS_WASAPI : undefined);
    const devices = rt.getDevices();
    const device = devices.find((d) => d.id === deviceId);
    if (!device) return { ok: false, error: `Device ${deviceId} not found` };

    const sampleRate = device.preferredSampleRate || 48000;
    const channels = Math.min(device.outputChannels, 2) || 2;
    const frameSize = 1920; // ~40ms @48kHz

    const liveWindows = () => BrowserWindow.getAllWindows().filter((w) => !w.isDestroyed());

    // Pass the output device id as inputParameters — WASAPI loopback mode.
    rt.openStream(
      null,
      { deviceId, nChannels: channels, firstChannel: 0 },
      RtAudioFormat.RTAUDIO_SINT16,
      sampleRate,
      frameSize,
      'OpenDropLoopback',
      (pcm) => liveWindows().forEach((w) => w.webContents.send('loopback:data', { sampleRate, channels, pcm })),
      null,
      0,
      (err) => liveWindows().forEach((w) => w.webContents.send('loopback:error', String(err)))
    );

    rt.start();
    loopbackRt = rt;
    return { ok: true, sampleRate, channels };
  } catch (e) {
    return { ok: false, error: e.message };
  }
});

ipcMain.handle('loopback:stop', async () => {
  if (loopbackRt) { try { loopbackRt.closeStream(); } catch {} loopbackRt = null; }
  return { ok: true };
});

// ── Custom app:// protocol for production SPA routing ─────────────────────
function registerProtocol() {
  protocol.handle('app', (request) => {
    const { pathname } = new URL(request.url);
    const rel = decodeURIComponent(pathname === '/' ? '/index.html' : pathname);
    const ext = path.extname(rel).toLowerCase();
    const candidate = path.join(BUILD_DIR, rel);
    const filePath = (ext && fs.existsSync(candidate))
      ? candidate
      : path.join(BUILD_DIR, 'index.html');
    try {
      const data = fs.readFileSync(filePath);
      const mime = MIME[path.extname(filePath).toLowerCase()] ?? 'application/octet-stream';
      return new Response(data, { headers: { 'Content-Type': mime } });
    } catch {
      return new Response('Not Found', { status: 404 });
    }
  });
}

function createWindow() {
  const win = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 900,
    minHeight: 600,
    backgroundColor: '#07071a',
    titleBarStyle: process.platform === 'darwin' ? 'hiddenInset' : 'default',
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
    },
    title: 'OpenDrop VJ',
    show: false,
  });

  win.once('ready-to-show', () => win.show());

  // ── System audio loopback via getDisplayMedia ──────────────────────────────
  // Intercepts renderer getDisplayMedia() calls. On Windows, fulfils with
  // native loopback (no screen-share dialog, no extra software). On macOS/Linux
  // the UI routes to Path B (device picker / BlackHole / .monitor) before
  // calling connectDisplay(), so this handler is a graceful fallback only.
  win.webContents.session.setDisplayMediaRequestHandler((_request, callback) => {
    // getDisplayMedia({ audio: true, video: true }) requires both streams to be
    // fulfilled — returning audio-only causes "Invalid capture constraints".
    // Always include a screen source so the video constraint is satisfied;
    // the renderer stops the video track immediately after capture.
    desktopCapturer.getSources({ types: ['screen'] }).then((sources) => {
      const video = sources[0] ?? null;
      if (!video) { callback({}); return; }
      if (process.platform === 'win32') {
        // Windows WASAPI loopback — captures all system audio output
        callback({ audio: 'loopback', video });
      } else {
        callback({ video });
      }
    }).catch(() => callback({}));
  }, { useSystemPicker: false });

  if (isDev) {
    win.loadURL(DEV_URL);
  } else {
    win.loadURL('app://localhost/');
  }

  // Track the output window for NDI capture
  win.webContents.on('did-create-window', (child) => {
    outputWin = child;
    child.on('closed', () => { if (outputWin === child) outputWin = null; });
  });

  // Allow output window opened via window.open('/output', ...)
  win.webContents.setWindowOpenHandler(() => ({
    action: 'allow',
    overrideBrowserWindowOptions: {
      width: 1280,
      height: 720,
      backgroundColor: '#000000',
      webPreferences: {
        preload: path.join(__dirname, 'preload.cjs'),
        contextIsolation: true,
        nodeIntegration: false,
      },
      title: 'OpenDrop — Output',
    },
  }));

  return win;
}

app.whenReady().then(() => {
  // Allow getUserMedia (audio device enumeration + capture) and display-capture
  // Without this, Electron may silently deny media permission requests, causing
  // enumerateDevices() to return only the default mic without labels.
  session.defaultSession.setPermissionRequestHandler((_wc, permission, callback) => {
    callback(['media', 'display-capture', 'mediaKeySystem'].includes(permission));
  });

  if (!isDev) registerProtocol();

  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (loopbackRt) { try { loopbackRt.closeStream(); } catch {} loopbackRt = null; }
  if (v4l2Proc) { try { v4l2Proc.stdin.end(); v4l2Proc.kill('SIGTERM'); } catch {} v4l2Proc = null; }
  if (process.platform !== 'darwin') app.quit();
});
