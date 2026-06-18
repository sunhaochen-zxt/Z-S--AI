import { contextBridge, ipcRenderer } from 'electron';

let port = 9786;
ipcRenderer.on('backend-port', (_e, p: number) => { port = p; });

contextBridge.exposeInMainWorld('electronAPI', {
  getBackendPort: () => port,
});
