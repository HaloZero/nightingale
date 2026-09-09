import type { Song } from '@/types/Song';

/**
 * A selection identity that survives the analyzer's file_hash rekey.
 *
 * Remote songs are scanned with a placeholder file_hash that flips to the real
 * Blake3 once analysis materialises the file, so tracking by file_hash loses the
 * row mid-analysis. `origin.item_id` is stable across that rekey; local paths are
 * unique even when duplicate files share a hash.
 */
export const songKey = (song: Song): string =>
  'item_id' in song.origin ? `${song.origin.kind}:${song.origin.item_id}` : song.path;
