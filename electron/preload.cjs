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

  // NDI input (find sources on the LAN + receive one as a video layer)
  ndiFind: () => ipcRenderer.invoke('ndi:find'),
  ndiReceiveStart: (name, urlAddress) => ipcRenderer.invoke('ndi:receiveStart', { name, urlAddress }),
  ndiReceiveStop: () => ipcRenderer.invoke('ndi:receiveStop'),
  onNdiFrame: (cb) => {
    const handler = (_, frame) => cb(frame);
    ipcRenderer.on('ndi:frame', handler);
    return () => ipcRenderer.off('ndi:frame', handler);
  },

  // v4l2loopback virtual webcam (Linux — requires ffmpeg + v4l2loopback kernel module)
  v4l2Start: () => ipcRenderer.invoke('v4l2:start'),
  v4l2Stop: () => ipcRenderer.invoke('v4l2:stop'),

  // Spout texture sharing (Windows — requires spout-addon + SpoutDX vendor sources)
  spoutStart: (name) => ipcRenderer.invoke('spout:start', { name }),
  spoutStop: () => ipcRenderer.invoke('spout:stop'),

  // Per-device output loopback (Windows, requires naudiodon-loopback)
  listOutputDevices: () => ipcRenderer.invoke('loopback:list'),
  startLoopback: (deviceId) => ipcRenderer.invoke('loopback:start', { deviceId }),
  stopLoopback: () => ipcRenderer.invoke('loopback:stop'),
  onLoopbackData: (cb) => {
    const handler = (_, data) => cb(data);
    ipcRenderer.on('loopback:data', handler);
    return () => ipcRenderer.off('loopback:data', handler);
  },

  // Raw PCM frames streamed from main renderer → output window for audio-reactivity.
  sendAudioFrame: (data) => ipcRenderer.send('audioframe:post', data),
  onAudioFrame: (cb) => {
    const handler = (_, data) => cb(data);
    ipcRenderer.on('audioframe:data', handler);
    return () => ipcRenderer.off('audioframe:data', handler);
  },

  // OSC UDP remote control (requires UDP port to be open on LAN)
  startOsc: (port) => ipcRenderer.invoke('osc:start', { port }),
  stopOsc: () => ipcRenderer.invoke('osc:stop'),
  onOscMsg: (cb) => {
    const handler = (_, cmdId, value01) => cb(cmdId, value01);
    ipcRenderer.on('osc:msg', handler);
    return () => ipcRenderer.off('osc:msg', handler);
  },

  // WebSocket remote control — phone/tablet touch UI at /remote
  startRemote: () => ipcRenderer.invoke('remote:start'),
  stopRemote: () => ipcRenderer.invoke('remote:stop'),
  onRemoteCmd: (cb) => {
    const handler = (_, cmd, value) => cb(cmd, value);
    ipcRenderer.on('remote:cmd', handler);
    return () => ipcRenderer.off('remote:cmd', handler);
  },

  // Ableton Link (optional — requires @ktamas77/abletonlink addon)
  startLink: (bpm) => ipcRenderer.invoke('link:start', { bpm }),
  stopLink: () => ipcRenderer.invoke('link:stop'),
  setLinkTempo: (bpm) => ipcRenderer.invoke('link:set-tempo', { bpm }),
  onLinkState: (cb) => {
    const handler = (_, state) => cb(state);
    ipcRenderer.on('link:state', handler);
    return () => ipcRenderer.off('link:state', handler);
  },

  // Screen targeting — open output fullscreen on a specific display
  listScreens: () => ipcRenderer.invoke('screen:list'),
  openOutputOnDisplay: (displayId) => ipcRenderer.invoke('output:open-on-display', displayId),
  onOutputWindowClosed: (cb) => {
    const handler = () => cb();
    ipcRenderer.on('output:window-closed', handler);
    return () => ipcRenderer.off('output:window-closed', handler);
  },

  // Secrets (OBS password, Twitch/Kick credentials) — write-only from the renderer's side.
  hasSecret: (key) => ipcRenderer.invoke('secrets:has', key),
  setSecret: (key, value) => ipcRenderer.invoke('secrets:set', { key, value }),
  clearSecret: (key) => ipcRenderer.invoke('secrets:clear', key),
});
