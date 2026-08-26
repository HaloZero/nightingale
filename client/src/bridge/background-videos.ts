import { invoke, listen, type UnlistenFn } from "./runtime";

// Server-only: the bulk Pixabay download and reel build both shell out to a
// vendored ffmpeg against the server's own data dir, so there's nothing for
// the Tauri desktop app to point at (see `showBackgroundVideos`/`!isTauri`
// in settings.tsx). No Tauri-side commands exist for these two names.

export interface PixabayBulkDownloadProgress {
  flavor: string;
  message: string;
}

export interface BackgroundReelsProgress {
  flavor: string;
  message: string;
}

/** Shared shape for both the cached-video and built-reel counts below. */
export interface CountAndCap {
  count: number;
  cap: number;
}

export const downloadAllPixabayVideos = async (flavor: string): Promise<void> => {
  return await invoke<void>("download_all_pixabay_videos", { flavor });
};

export const getBackgroundVideoCount = async (flavor: string): Promise<CountAndCap> => {
  return await invoke<CountAndCap>("get_background_video_count", { flavor });
};

export const onPixabayBulkDownloadProgress = async (
  cb: (progress: PixabayBulkDownloadProgress) => void,
): Promise<UnlistenFn> => {
  return await listen<PixabayBulkDownloadProgress>(
    "pixabay-bulk-download-progress",
    ({ payload }) => cb(payload),
  );
};

export const onPixabayBulkDownloadDone = async (
  cb: (result: { flavor: string }) => void,
): Promise<UnlistenFn> => {
  return await listen<{ flavor: string }>("pixabay-bulk-download-done", ({ payload }) =>
    cb(payload),
  );
};

export const buildBackgroundReels = async (flavor: string): Promise<void> => {
  return await invoke<void>("build_background_reels", { flavor });
};

export const getBackgroundReelCount = async (flavor: string): Promise<CountAndCap> => {
  return await invoke<CountAndCap>("get_background_reel_count", { flavor });
};

export const onBackgroundReelsProgress = async (
  cb: (progress: BackgroundReelsProgress) => void,
): Promise<UnlistenFn> => {
  return await listen<BackgroundReelsProgress>("background-reels-progress", ({ payload }) =>
    cb(payload),
  );
};

export const onBackgroundReelsDone = async (
  cb: (result: { flavor: string }) => void,
): Promise<UnlistenFn> => {
  return await listen<{ flavor: string }>("background-reels-done", ({ payload }) => cb(payload));
};
