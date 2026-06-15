'use strict';

const { app, BrowserWindow, ipcMain, desktopCapturer, protocol, net } = require('electron');
const path = require('path');
const fs = require('fs');

// ── NDI (optional — requires NDI SDK + grandiose) ──────────────────────────
let grandiose = null;
let ndiSender = null;
let ndiTimer = null;
let outputWin = null;     // tracked below in did-create-window

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

// ── Custom app:// protocol for production SPA routing ─────────────────────
function registerProtocol() {
  protocol.handle('app', async (request) => {
    const { pathname } = new URL(request.url);
    const ext = path.extname(pathname);
    const candidate = path.join(BUILD_DIR, pathname === '/' ? 'index.html' : pathname);
    if (ext && fs.existsSync(candidate)) {
      return net.fetch(`file://${candidate}`);
    }
    // SPA fallback — let the client router handle the route
    return net.fetch(`file://${path.join(BUILD_DIR, 'index.html')}`);
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
    if (process.platform === 'win32') {
      callback({ audio: 'loopback' });
      return;
    }
    desktopCapturer.getSources({ types: ['screen'] })
      .then((sources) => callback(sources.length ? { video: sources[0] } : {}))
      .catch(() => callback({}));
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
  if (!isDev) registerProtocol();

  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
