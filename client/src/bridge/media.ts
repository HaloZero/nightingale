import { convertFileSrc as tauriConvertFileSrc } from '@tauri-apps/api/core';

import { isTauri } from './runtime';

/**
 * Tauri exposes filesystem paths to the webview via the `asset://` protocol.
 * The web server mirrors this with `/api/asset?path=...`, which canonicalises
 * the path against the allowed data roots before serving.
 */
export const convertFileSrc = (path: string): string => {
  if (!path) {
    return '';
  }
  if (isTauri) {
    return tauriConvertFileSrc(path);
  }
  return `/api/asset?path=${encodeURIComponent(path)}`;
};
