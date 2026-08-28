import React from "react";
import ReactDOM from "react-dom/client";
// App.tsx is the only other place this gets imported -- since receiver-main
// is its own Vite entry (not part of the App.tsx module graph), the
// Tailwind build (`@import "tailwindcss"` etc., see App.css) never reaches
// this bundle without importing it here too. Without this, every Tailwind
// utility class used by the reused Background/LyricsDisplay components
// (fixed inset-0, absolute z-10, ...) is a no-op -- elements exist and
// video/audio play, but nothing is positioned or visible.
import "./App.css";
import { ReceiverApp } from "./pages/receiver/receiver-app";

// Always web (a Cast receiver is a plain Chromium tab, never Tauri) --
// unlike main.tsx there's no bootstrap-preload dance or Tauri branch, just
// a straight mount. loadConfig()/loadSongsByHashes() work standalone via
// invoke("load_config"/"load_songs_by_hashes") regardless.
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ReceiverApp />
  </React.StrictMode>,
);
