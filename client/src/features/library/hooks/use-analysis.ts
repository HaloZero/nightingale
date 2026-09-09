import { type InfiniteData, useQueryClient } from '@tanstack/react-query';
import { useMemo } from 'react';
import { toast } from 'sonner';

import {
  cancelAnalysis,
  deleteSongCache,
  enqueueAll,
  enqueueOne,
  realign,
  realignAlt,
  realignAll,
  reanalyzeAllForceTranscribe,
  reanalyzeAllFull,
  reanalyzeAllTranscript,
  reanalyzeForceTranscribe,
  reanalyzeFull,
  reanalyzeTranscript,
  refreshMetadata,
  refreshMetadataAll,
  removeFromQueue,
  removeFromQueueAll,
  songsByFilter,
  songsByHashes,
} from '@/bridge/analysis';
import { bestKaraokeVideoAll, forceBestKaraokeVideoAll } from '@/bridge/karaoke-video';
import { useLibraryFilter } from '@/features/menu/hooks/use-library-filter';
import { useSearch } from '@/features/menu/hooks/use-search';
import { ANALYSIS_QUEUE, MENU, SONGS, SONGS_META } from '@/shared/query-keys';
import type { LibraryMenuFilters } from '@/types/LibraryMenuFilters';
import type { Song } from '@/types/Song';
import type { SongsStore } from '@/types/SongsStore';

const withoutAnalysisCache = (song: Song): Song => ({
  ...song,
  is_analyzed: false,
  language: null,
  transcript_source: null,
  key: null,
  override_key: null,
  tempo: 1,
  key_offset: 0,
  no_stems: false,
});

export const useAnalysis = () => {
  const queryClient = useQueryClient();
  const { artist, album, genre, playlist, query, status, transcript_source, language } =
    useLibraryFilter();
  const { search } = useSearch();

  return useMemo(() => {
    const currentFilters = (): LibraryMenuFilters => ({
      artist,
      album,
      genre,
      playlist,
      query,
      status,
      transcript_source,
      language,
      search: search || null,
    });

    const one = (fileHash: string) => songsByHashes([fileHash]);
    const filtered = () => songsByFilter(currentFilters());

    const invalidateQueue = () => {
      void queryClient.invalidateQueries({ queryKey: ANALYSIS_QUEUE });
    };

    const invalidateSongs = () => {
      void queryClient.invalidateQueries({ queryKey: MENU });
      void queryClient.invalidateQueries({ queryKey: SONGS });
      void queryClient.invalidateQueries({ queryKey: SONGS_META });
      void queryClient.invalidateQueries({ queryKey: ANALYSIS_QUEUE });
    };

    const markSongCacheDeleted = (fileHash: string) => {
      queryClient.setQueriesData<InfiniteData<SongsStore>>(
        { queryKey: SONGS },
        (data) =>
          data && {
            ...data,
            pages: data.pages.map((page) => ({
              ...page,
              processed: page.processed.map((song) =>
                song.file_hash === fileHash ? withoutAnalysisCache(song) : song,
              ),
            })),
          },
      );
    };

    const wrap =
      <A extends unknown[]>(handler: (...args: A) => Promise<void>, invalidate: () => void) =>
      async (...args: A) => {
        try {
          await handler(...args);
          invalidate();
        } catch (error: unknown) {
          toast.error(
            `Error while running an analysis action: ${error instanceof Error ? error.message : 'unknown error'}`,
          );
        }
      };

    // Same as `wrap`, but for the bulk actions: they resolve with how many
    // eligible songs got queued (ineligible ones -- not yet analyzed, USDX,
    // etc. depending on the action -- are excluded server-side, never
    // counted at all), so report that instead of a generic success.
    const wrapBulk =
      <A extends unknown[]>(
        label: string,
        handler: (...args: A) => Promise<number>,
        invalidate: () => void,
      ) =>
      async (...args: A) => {
        try {
          const count = await handler(...args);
          invalidate();
          if (count > 0) {
            toast.success(`Queued ${count} song${count === 1 ? '' : 's'} for ${label}`);
          } else {
            toast.info(`No eligible songs for ${label} in the current filter`);
          }
        } catch (error: unknown) {
          toast.error(
            `Error while running a bulk analysis action: ${error instanceof Error ? error.message : 'unknown error'}`,
          );
        }
      };

    // Same shape as wrapBulk, but for actions that finish synchronously
    // (removeFromQueueAll doesn't touch the analysis queue asynchronously --
    // it's done by the time it resolves) rather than queuing work -- "Queued
    // N songs for..." would be misleading since the work is already done.
    const wrapBulkDone =
      <A extends unknown[]>(
        label: string,
        handler: (...args: A) => Promise<number>,
        invalidate: () => void,
      ) =>
      async (...args: A) => {
        try {
          const count = await handler(...args);
          invalidate();
          if (count > 0) {
            toast.success(`${label} for ${count} song${count === 1 ? '' : 's'}`);
          } else {
            toast.info(`No eligible songs for ${label.toLowerCase()} in the current filter`);
          }
        } catch (error: unknown) {
          toast.error(
            `Error while running a bulk analysis action: ${error instanceof Error ? error.message : 'unknown error'}`,
          );
        }
      };

    return {
      enqueueOne: wrap(enqueueOne, invalidateQueue),
      enqueueAll: wrap(() => enqueueAll(currentFilters()), invalidateQueue),
      removeFromQueue: wrap(removeFromQueue, invalidateQueue),
      cancelAnalysisOne: wrapBulkDone(
        'Cancelled analysis',
        (fileHash: string) => cancelAnalysis(one(fileHash)),
        invalidateSongs,
      ),
      cancelAnalysisAll: wrapBulkDone(
        'Cancelled analysis',
        () => cancelAnalysis(filtered()),
        invalidateSongs,
      ),
      deleteSongCache: wrap(async (fileHash: string) => {
        await deleteSongCache(fileHash);
        markSongCacheDeleted(fileHash);
      }, invalidateSongs),
      reanalyzeTranscript: wrap(reanalyzeTranscript, invalidateSongs),
      reanalyzeFull: wrap(reanalyzeFull, invalidateSongs),
      realign: wrap(realign, invalidateSongs),
      realignAlt: wrap(realignAlt, invalidateSongs),
      reanalyzeForceTranscribe: wrap(reanalyzeForceTranscribe, invalidateSongs),
      refreshMetadata: wrap(refreshMetadata, invalidateSongs),
      refreshMetadataAll: wrapBulk(
        'metadata refresh',
        () => refreshMetadataAll(currentFilters()),
        invalidateSongs,
      ),
      removeFromQueueAll: wrapBulkDone(
        'Removed from queue',
        () => removeFromQueueAll(currentFilters()),
        invalidateSongs,
      ),
      reanalyzeAllFull: wrapBulk(
        'full reanalysis',
        () => reanalyzeAllFull(currentFilters()),
        invalidateSongs,
      ),
      reanalyzeAllTranscript: wrapBulk(
        'refetching lyrics & aligning',
        (lang?: string) => reanalyzeAllTranscript(currentFilters(), lang),
        invalidateSongs,
      ),
      reanalyzeAllForceTranscribe: wrapBulk(
        'force transcribing',
        () => reanalyzeAllForceTranscribe(currentFilters()),
        invalidateSongs,
      ),
      realignAll: wrapBulk(
        'realigning',
        (lang?: string) => realignAll(currentFilters(), lang),
        invalidateSongs,
      ),
      bestKaraokeVideoAll: wrapBulk(
        'karaoke video rendering',
        () => bestKaraokeVideoAll(currentFilters()),
        invalidateSongs,
      ),
      forceBestKaraokeVideoAll: wrapBulk(
        'karaoke video re-rendering',
        () => forceBestKaraokeVideoAll(currentFilters()),
        invalidateSongs,
      ),
    };
  }, [
    queryClient,
    artist,
    album,
    genre,
    playlist,
    query,
    status,
    transcript_source,
    language,
    search,
  ]);
};
