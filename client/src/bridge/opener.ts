import { openUrl as tauriOpenUrl } from '@tauri-apps/plugin-opener';

import { isTauri } from './runtime';

export const openUrl = async (url: string): Promise<void> => {
  if (isTauri) {
    await tauriOpenUrl(url);
    return;
  }

  window.open(url, '_blank', 'noopener,noreferrer');
};
