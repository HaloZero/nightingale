import { invoke, listen, type UnlistenFn } from "./runtime";

// Server-only: rendering shells out to a vendored ffmpeg against the
// server's own data dir (same as casting itself), so there's nothing for
// the Tauri desktop app to point at -- no Tauri-side commands exist for
// either of these two names.

export interface KaraokeVideoReady {
  file_hash: string;
  error: string | null;
}

/** No-op if `fileHash` already has a video that's fresh relative to its transcript. */
export const renderKaraokeVideo = async (fileHash: string): Promise<void> => {
  return await invoke<void>("render_karaoke_video", { fileHash });
};

/** Always regenerates, even if a fresh video already exists -- also picks a new random background. */
export const forceRerenderKaraokeVideo = async (fileHash: string): Promise<void> => {
  return await invoke<void>("force_rerender_karaoke_video", { fileHash });
};

export const onKaraokeVideoReady = async (
  cb: (event: KaraokeVideoReady) => void,
): Promise<UnlistenFn> => {
  return await listen<KaraokeVideoReady>("karaoke-video-ready", ({ payload }) => cb(payload));
};
