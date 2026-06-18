interface ElectronAPI {
  getBackendPort: () => Promise<number>;
}

interface Window {
  electronAPI?: ElectronAPI;
}
