/**
 * Context-isolated Optimus renderer bridge.
 * No bearer token, Node primitive, or remote-page preload crosses this boundary.
 */
const { contextBridge, ipcRenderer } = require('electron');

function subscribe(channel, listener) {
  if (typeof listener !== 'function') throw new TypeError('listener must be a function');
  const wrapped = (_event, payload) => listener(payload);
  ipcRenderer.on(channel, wrapped);
  return () => ipcRenderer.removeListener(channel, wrapped);
}

contextBridge.exposeInMainWorld('optimusElectron', {
  isElectron: true,
  // Temporary compatibility surface. React never calls this in production and
  // main omits the token outside explicit legacy mode.
  hostInfo: () => ipcRenderer.invoke('optimus:host-info'),
  invoke: (method, params) => ipcRenderer.invoke('optimus:invoke', method, params || {}),
  chat: {
    start: (request) => ipcRenderer.invoke('optimus:chat-start', request),
    cancel: (streamId) => ipcRenderer.invoke('optimus:chat-cancel', streamId),
    subscribe: (listener) => subscribe('optimus:chat-event', listener),
  },
  browser: {
    setBounds: (bounds) => ipcRenderer.send('optimus:browser-bounds', bounds),
    setVisible: (visible) => ipcRenderer.send('optimus:browser-visible', Boolean(visible)),
    navigate: (url) => ipcRenderer.invoke('optimus:browser-navigate', url),
    back: () => ipcRenderer.invoke('optimus:browser-back'),
    forward: () => ipcRenderer.invoke('optimus:browser-forward'),
    reload: () => ipcRenderer.invoke('optimus:browser-reload'),
    state: () => ipcRenderer.invoke('optimus:browser-state'),
    subscribe: (listener) => subscribe('optimus:browser-state', listener),
  },
  windowAction: (action) => ipcRenderer.invoke('optimus:window', action),
  pickFolder: () => ipcRenderer.invoke('optimus:pick-folder'),
  openPath: (targetPath) => ipcRenderer.invoke('optimus:open-path', targetPath),
  openUrl: (url) => ipcRenderer.invoke('optimus:open-url', url),
});

// Retained only for the rollback shell. New React code uses optimusElectron.
contextBridge.exposeInMainWorld('optimusHost', {
  info: () => ipcRenderer.invoke('optimus:host-info'),
  window: (action) => ipcRenderer.invoke('optimus:window', action),
  pickFolder: () => ipcRenderer.invoke('optimus:pick-folder'),
  openPath: (targetPath) => ipcRenderer.invoke('optimus:open-path', targetPath),
  openUrl: (url) => ipcRenderer.invoke('optimus:open-url', url),
});
