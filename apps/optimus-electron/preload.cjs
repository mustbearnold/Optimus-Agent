/**
 * Preload: context-isolated bridge for Optimus UI.
 * Exposes window.optimusHost + window.optimus (invoke-compatible).
 */
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('optimusHost', {
  info: () => ipcRenderer.invoke('optimus:host-info'),
  window: (action) => ipcRenderer.invoke('optimus:window', action),
  pickFolder: () => ipcRenderer.invoke('optimus:pick-folder'),
  openPath: (p) => ipcRenderer.invoke('optimus:open-path', p),
  openUrl: (url) => ipcRenderer.invoke('optimus:open-url', url),
});

// Lightweight optimus client for React (and optional legacy override).
contextBridge.exposeInMainWorld('optimusElectron', {
  isElectron: true,
  hostInfo: () => ipcRenderer.invoke('optimus:host-info'),
  windowAction: (action) => ipcRenderer.invoke('optimus:window', action),
  pickFolder: () => ipcRenderer.invoke('optimus:pick-folder'),
  openPath: (p) => ipcRenderer.invoke('optimus:open-path', p),
  openUrl: (url) => ipcRenderer.invoke('optimus:open-url', url),
});
