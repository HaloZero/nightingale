/**
 * Operator-controlled deployment flags surfaced by the self-hosted server via
 * `/api/bootstrap` (seeded into `window.__NIGHTINGALE_SERVER_FLAGS__` before
 * React mounts). The Tauri desktop build never sets these, so both default to
 * `false` there.
 */
export type ServerFlags = {
  /** Data folder is fixed via `NIGHTINGALE_DATA_PATH`; hide the data picker. */
  dataPathPinned: boolean;
  /** Library folder is fixed via `NIGHTINGALE_LIBRARY_PATH`; hide folder-select. */
  libraryPinned: boolean;
};

const defaults: ServerFlags = { dataPathPinned: false, libraryPinned: false };

export const getServerFlags = (): ServerFlags => {
  if (typeof window === 'undefined') {
    return defaults;
  }

  return window.__NIGHTINGALE_SERVER_FLAGS__ ?? defaults;
};
