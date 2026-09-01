import type { LibraryMenuFilters } from "@/types/LibraryMenuFilters";
import { invoke, listen, type UnlistenFn } from "./runtime";

// Server-only: rendering shells out to a vendored ffmpeg against the
// server's own data dir (same as casting itself), so there's nothing for
// the Tauri desktop app to point at -- no Tauri-side commands exist for
// either of these two names.

export interface KaraokeVideoReady {
  file_hash: string;
  /** Whether TheAudioDB actually had a music video for this song at all --
   * `false` means the render fell back to a reel background instead. */
  music_video_found: boolean;
  error: string | null;
}

/**
 * The single "get me a karaoke video" action: always tries a
 * YouTube-background render first (TheAudioDB lookup, cached server-side --
 * yt-dlp download, skipped if already downloaded -- then render), falling
 * back to a reel-background render only if no YouTube-backed video could be
 * produced. No-ops if a fresh YouTube-background render already exists;
 * still tries a fresh reel render if only a stale/missing one is on hand.
 */
export const bestKaraokeVideo = async (fileHash: string): Promise<void> => {
  return await invoke<void>("best_karaoke_video", { fileHash });
};

/**
 * Same as `bestKaraokeVideo` but clears both cached flavors first and
 * regenerates unconditionally -- the "no really, redo this one" button, for
 * a bad download, a stale render, or picking a fresh random reel background.
 */
export const forceBestKaraokeVideo = async (fileHash: string): Promise<void> => {
  return await invoke<void>("force_best_karaoke_video", { fileHash });
};

export const onKaraokeVideoReady = async (
  cb: (event: KaraokeVideoReady) => void,
): Promise<UnlistenFn> => {
  return await listen<KaraokeVideoReady>("karaoke-video-ready", ({ payload }) => cb(payload));
};

// ─── Bulk (filtered-library) counterparts ──────────────────────────────────
// Same eligibility as the per-song actions (already analyzed, not USDX --
// see iter_file_hashes_filtered_karaoke_renderable), resolved server-side
// from `filters`. Each runs sequentially on a background thread server-side
// and resolves immediately with how many songs were queued -- individual
// completions/failures aren't tracked back to the frontend per song, only
// the per-song "karaoke-video-ready" event fires for whichever song's
// sidebar happens to be open when its turn comes up.

/** No-ops per-song for anything already fresh, same as the single-song action. */
export const bestKaraokeVideoAll = async (filters: LibraryMenuFilters): Promise<number> => {
  return await invoke<number>("best_karaoke_video_all", { filters });
};

/** Clears and regenerates both flavors unconditionally, same as the single-song action. */
export const forceBestKaraokeVideoAll = async (filters: LibraryMenuFilters): Promise<number> => {
  return await invoke<number>("force_best_karaoke_video_all", { filters });
};
