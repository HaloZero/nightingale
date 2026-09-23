import type { AnalysisQueue } from '@/types/AnalysisQueue';
import type { LoadSongsParams } from '@/types/LoadSongsParams';
import type { Song } from '@/types/Song';
import type { SongsMeta } from '@/types/SongsMeta';
import type { SongsStore } from '@/types/SongsStore';
import type { VideoProcessingQueue } from '@/types/VideoProcessingQueue';

import { invoke } from './runtime';

export function getPreloadedSongsMeta(): SongsMeta | undefined {
  if (typeof window === 'undefined') {
    return undefined;
  }
  return window.__NIGHTINGALE_SONGS_META__;
}

export const loadSongs = async (params: LoadSongsParams): Promise<SongsStore> => {
  return await invoke<SongsStore>('load_songs', { params });
};

export const loadSongsByHashes = async (fileHashes: string[]): Promise<Song[]> => {
  return await invoke<Song[]>('load_songs_by_hashes', { fileHashes });
};

/**
 * Ranked, typo-tolerant free-text search across the whole library (any
 * origin, any analysis status). Backed by the same similarity scoring as
 * Chromecast voice matching -- unlike `loadSongs`'s plain substring search
 * used by the main library browser, which is intentionally left alone.
 */
export const searchSongs = async (query: string, limit: number): Promise<Song[]> => {
  return await invoke<Song[]>('search_songs', { query, limit });
};

export const loadSongsMeta = async (): Promise<SongsMeta> => {
  return await invoke<SongsMeta>('load_songs_meta');
};

export const loadAnalysisQueue = async (): Promise<AnalysisQueue> => {
  return await invoke<AnalysisQueue>('load_analysis_queue');
};

/** Server-only, like the rest of the karaoke/YouTube-video bridge -- see `bridge/karaoke-video.ts`. */
export const loadVideoQueue = async (): Promise<VideoProcessingQueue> => {
  return await invoke<VideoProcessingQueue>('load_video_queue');
};
