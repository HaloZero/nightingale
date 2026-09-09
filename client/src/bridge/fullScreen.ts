import { getCurrentWindow } from '@tauri-apps/api/window';

import { windowImmersive } from '@/bridge/window';

import { isTauri } from './runtime';

let cachedTauriWindow: ReturnType<typeof getCurrentWindow> | null = null;

const tauriWindow = () => {
  if (cachedTauriWindow === null) {
    cachedTauriWindow = getCurrentWindow();
  }
  return cachedTauriWindow;
};

export const isFullScreen = (): Promise<boolean> => {
  if (!isTauri) {
    return Promise.resolve(typeof document !== 'undefined' && document.fullscreenElement !== null);
  }
  return windowImmersive();
};

export const setFullScreen = async (next: boolean): Promise<void> => {
  if (!isTauri) {
    if (typeof document === 'undefined') {
      return;
    }
    if (next) {
      try {
        await document.documentElement.requestFullscreen();
      } catch {
        // User-gesture or permissions can deny; the UI tolerates a stale flag.
      }
    } else if (document.fullscreenElement !== null) {
      try {
        await document.exitFullscreen();
      } catch {
        // Same as above.
      }
    }
    return;
  }
  await tauriWindow().setSimpleFullscreen(next);
};
