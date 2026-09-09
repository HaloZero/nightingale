import { platform } from '@tauri-apps/plugin-os';

import { isTauri } from './runtime';

/**
 * Update delivery channel for the running build.
 *
 *  - `auto`            — Tauri non-Linux build. The Tauri updater handles
 *                        download + restart in-app.
 *  - `linux-tauri`     — Tauri Linux build. No bundler-supported auto-update,
 *                        so the user manually grabs the next release from
 *                        GitHub.
 *  - `self-hosted-web` — Web target served by the self-hosted server. The
 *                        operator updates the binary by re-running the
 *                        install script on the host.
 *
 * `platform()` queries Tauri's OS plugin which throws outside the webview,
 * so the `!isTauri` short-circuit MUST come first; otherwise the web build
 * would crash at module-eval.
 */
export type UpdateChannel = 'auto' | 'linux-tauri' | 'self-hosted-web';

const updateChannel = (): UpdateChannel => {
  if (!isTauri) {
    return 'self-hosted-web';
  }
  return platform() === 'linux' ? 'linux-tauri' : 'auto';
};

export const UPDATE_CHANNEL = updateChannel();

export const UPDATES_SUPPORTED: boolean = UPDATE_CHANNEL === 'auto';
