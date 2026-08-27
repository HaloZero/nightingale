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

export interface YoutubeKaraokeVideoReady {
  file_hash: string;
  /** Whether TheAudioDB actually had a music video for this song at all. */
  music_video_found: boolean;
  error: string | null;
}

/**
 * Chains the TheAudioDB lookup (cached server-side, so repeat calls don't
 * re-hit the API), the video download, and a forced karaoke-video
 * re-render into one action. `music_video_found: false` on the resulting
 * event means no video exists to use -- the reel background stays as-is.
 */
export const fetchYoutubeKaraokeVideo = async (fileHash: string): Promise<void> => {
  return await invoke<void>("fetch_youtube_karaoke_video", { fileHash });
};

export const onYoutubeKaraokeVideoReady = async (
  cb: (event: YoutubeKaraokeVideoReady) => void,
): Promise<UnlistenFn> => {
  return await listen<YoutubeKaraokeVideoReady>("youtube-karaoke-video-ready", ({ payload }) =>
    cb(payload),
  );
};
