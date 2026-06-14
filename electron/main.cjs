'use strict';

const { app, BrowserWindow, ipcMain, desktopCapturer, protocol, net } = require('electron');
const path = require('path');
const fs = require('fs');

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

// ── Loopback audio: expose desktopCapturer sources to renderer ─────────────
ipcMain.handle('get-loopback-sources', async () => {
  try {
    const sources = await desktopCapturer.getSources({ types: ['screen'] });
    return sources.map((s) => ({ id: s.id, name: s.name }));
  } catch {
    return [];
  }
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

  if (isDev) {
    win.loadURL(DEV_URL);
  } else {
    win.loadURL('app://localhost/');
  }

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
