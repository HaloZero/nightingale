import { relaunch } from '@tauri-apps/plugin-process';
import { check, type Update, type DownloadEvent } from '@tauri-apps/plugin-updater';

import { isTauri } from './runtime';

const CHECK_TIMEOUT_MS = 30_000;

export const checkForUpdate = async (): Promise<Update | null> => {
  if (!isTauri) {
    return null;
  }
  return await check({ timeout: CHECK_TIMEOUT_MS });
};

export const downloadAndInstallUpdate = async (
  update: Update,
  onProgress: (event: DownloadEvent) => void,
): Promise<void> => {
  if (!isTauri) {
    return;
  }
  await update.downloadAndInstall(onProgress);
};

export const relaunchApp = async (): Promise<void> => {
  if (!isTauri) {
    window.location.reload();
    return;
  }
  await relaunch();
};
