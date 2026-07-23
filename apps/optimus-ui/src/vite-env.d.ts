/// <reference types="vite/client" />

interface Window {
  optimusElectron?: {
    isElectron: boolean;
    hostInfo: () => Promise<{ baseUrl: string; token: string; uiMode?: string }>;
    windowAction: (action: string) => Promise<unknown>;
    pickFolder: () => Promise<unknown>;
    openPath: (p: string) => Promise<unknown>;
    openUrl: (url: string) => Promise<unknown>;
  };
  __OPTIMUS_HTTP_TOKEN__?: string;
  __OPTIMUS_HTTP_MODE__?: boolean;
}
