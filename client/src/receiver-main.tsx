import React from "react";
import ReactDOM from "react-dom/client";
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
