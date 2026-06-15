'use strict';

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  isElectron: true,

  getPlatform: () => ipcRenderer.invoke('get-platform'),

  // BroadcastChannel relay through main process (cross-window IPC)
  sendBroadcast: (data) => ipcRenderer.send('bc-post', data),
  onBroadcast: (cb) => {
    const handler = (_, data) => cb(data);
    ipcRenderer.on('bc-msg', handler);
    return () => ipcRenderer.off('bc-msg', handler);
  },

  // NDI output (requires NDI SDK + grandiose)
  ndiStart: (name, width, height) => ipcRenderer.invoke('ndi:start', { name, width, height }),
  ndiStop: () => ipcRenderer.invoke('ndi:stop'),
});
