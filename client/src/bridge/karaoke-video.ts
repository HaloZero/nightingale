import type { LibraryMenuFilters } from "@/types/LibraryMenuFilters";
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
 * re-hit the API), the video download (skipped if already downloaded), and
 * the karaoke-video render into one action. No-ops the whole chain if a
 * YouTube-background render already exists and is fresh relative to the
 * transcript -- same freshness check as the reel-background action, so
 * re-running this on an already-fetched song does nothing. `music_video_found:
 * false` on the resulting event means no video exists to use -- the reel
 * background stays as-is.
 */
export const fetchYoutubeKaraokeVideo = async (fileHash: string): Promise<void> => {
  return await invoke<void>("fetch_youtube_karaoke_video", { fileHash });
};

/**
 * Same as `fetchYoutubeKaraokeVideo` but always redoes the download and
 * render, discarding whatever's cached -- for a bad download or a stale
 * render, not a wrong match (the AudioDB lookup itself is still served from
 * its own cache either way). The "no really, redo this one" counterpart,
 * same role `forceRerenderKaraokeVideo` plays for the reel-background video.
 */
export const forceFetchYoutubeKaraokeVideo = async (fileHash: string): Promise<void> => {
  return await invoke<void>("force_fetch_youtube_karaoke_video", { fileHash });
};

export const onYoutubeKaraokeVideoReady = async (
  cb: (event: YoutubeKaraokeVideoReady) => void,
): Promise<UnlistenFn> => {
  return await listen<YoutubeKaraokeVideoReady>("youtube-karaoke-video-ready", ({ payload }) =>
    cb(payload),
  );
};

// ─── Bulk (filtered-library) counterparts ──────────────────────────────────
// Same eligibility as the per-song actions (already analyzed, not USDX --
// see iter_file_hashes_filtered_karaoke_renderable), resolved server-side
// from `filters`. Each runs sequentially on a background thread server-side
// and resolves immediately with how many songs were queued -- individual
// completions/failures aren't tracked back to the frontend per song, only
// the per-song "*-ready" events fire for whichever song's sidebar happens
// to be open when its turn comes up.

/** No-ops per-song for anything already fresh, same as the single-song action. */
export const renderKaraokeVideoAll = async (filters: LibraryMenuFilters): Promise<number> => {
  return await invoke<number>("render_karaoke_video_all", { filters });
};

export const forceRerenderKaraokeVideoAll = async (
  filters: LibraryMenuFilters,
): Promise<number> => {
  return await invoke<number>("force_rerender_karaoke_video_all", { filters });
};

/**
 * Runs the lookup -> download -> render chain per eligible song, skipping
 * songs that already have a fresh YouTube-background render. TheAudioDB
 * lookups and downloads are both throttled server-side, so a large filtered
 * set with a cold cache (or many songs still needing a fetch) is slow by
 * design (staying under TheAudioDB's free tier / being polite to YouTube),
 * not stuck.
 */
export const fetchYoutubeKaraokeVideoAll = async (filters: LibraryMenuFilters): Promise<number> => {
  return await invoke<number>("fetch_youtube_karaoke_video_all", { filters });
};
