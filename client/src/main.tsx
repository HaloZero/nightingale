import React from 'react';
import ReactDOM from 'react-dom/client';

import { App } from '@/app/app';
import { isTauri } from '@/bridge/runtime';
import { webBootstrapSchema } from '@/bridge/schemas';

document.addEventListener('contextmenu', (e) => e.preventDefault());

/**
 * Tauri injects `window.__NIGHTINGALE_*` ahead of script execution via the
 * webview `initialization_script`. The web target gets the same payload from
 * `/api/bootstrap` and seeds the globals before React mounts so downstream
 * code can read them synchronously.
 */
async function loadWebBootstrap(): Promise<void> {
  if (isTauri) {
    return;
  }

  try {
    const res = await fetch('/api/bootstrap');
    if (!res.ok) {
      return;
    }
    const data: unknown = await res.json();
    const bootstrap = webBootstrapSchema.parse(data);
    window.__NIGHTINGALE_APP_CONFIG__ = bootstrap.config;
    window.__NIGHTINGALE_SONGS_META__ = bootstrap.songsMeta;
    window.__NIGHTINGALE_SERVER_FLAGS__ = {
      dataPathPinned: Boolean(bootstrap.dataPathPinned),
      libraryPinned: Boolean(bootstrap.libraryPinned),
    };
  } catch {}
}

void loadWebBootstrap().then(() => {
  const root = document.getElementById('root');
  if (root === null) {
    throw new Error('Missing application root');
  }
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
  return undefined;
});
