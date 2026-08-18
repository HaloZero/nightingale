import { ANALYSIS_QUEUE, MENU, SONGS, SONGS_META } from "@/queries/keys";
import { useLibraryFilter } from "@/hooks/use-library-filter";
import { useSearch } from "@/hooks/use-search";
import {
  deleteSongCache,
  deleteSongCacheAll,
  enqueueAll,
  enqueueOne,
  realign,
  realignAll,
  reanalyzeAllForceTranscribe,
  reanalyzeAllFull,
  reanalyzeAllTranscript,
  reanalyzeForceTranscribe,
  reanalyzeFull,
  reanalyzeTranscript,
  refreshMetadata,
  refreshMetadataAll,
} from "@/bridge/analysis";
import type { LibraryMenuFilters } from "@/types/LibraryMenuFilters";
import type { Song } from "@/types/Song";
import type { SongsStore } from "@/types/SongsStore";
import { type InfiniteData, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { toast } from "sonner";

enum BulkActionKind {
  Queued,
  Immediate,
}

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
  const { artist, album, playlist, query, status, transcript_source } = useLibraryFilter();
  const { search } = useSearch();

  return useMemo(() => {
    const currentFilters = (): LibraryMenuFilters => ({
      artist,
      album,
      playlist,
      query,
      status,
      transcript_source,
      search: search || null,
    });

    const invalidateQueue = () => {
      queryClient.invalidateQueries({ queryKey: ANALYSIS_QUEUE });
    };

    const invalidateSongs = () => {
      queryClient.invalidateQueries({ queryKey: MENU });
      queryClient.invalidateQueries({ queryKey: SONGS });
      queryClient.invalidateQueries({ queryKey: SONGS_META });
      queryClient.invalidateQueries({ queryKey: ANALYSIS_QUEUE });
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
            `Error while running an analysis action: ${error instanceof Error ? error.message : "unknown error"}`,
          );
        }
      };

    const wrapResult =
      <A extends unknown[], R>(handler: (...args: A) => Promise<R>, invalidate: () => void) =>
      async (...args: A): Promise<R | undefined> => {
        try {
          const result = await handler(...args);
          invalidate();
          return result;
        } catch (error: unknown) {
          toast.error(
            `Error while running an analysis action: ${error instanceof Error ? error.message : "unknown error"}`,
          );
          return undefined;
        }
      };

    const wrapBulk =
      <A extends unknown[]>(
        kind: BulkActionKind,
        label: string,
        handler: (...args: A) => Promise<number>,
        invalidate: () => void,
      ) =>
      async (...args: A) => {
        try {
          const count = await handler(...args);
          invalidate();
          if (count > 0) {
            toast.success(
              kind === BulkActionKind.Queued
                ? `Queued ${count} song${count === 1 ? "" : "s"} for ${label}`
                : `${label} for ${count} song${count === 1 ? "" : "s"}`,
            );
          } else {
            toast.info(
              `No eligible songs for ${kind === BulkActionKind.Queued ? label : label.toLowerCase()} in the current filter`,
            );
          }
        } catch (error: unknown) {
          toast.error(
            `Error while running a bulk analysis action: ${error instanceof Error ? error.message : "unknown error"}`,
          );
        }
      };

    return {
      enqueueOne: wrap(enqueueOne, invalidateQueue),
      enqueueAll: wrap(() => enqueueAll(currentFilters()), invalidateQueue),
      deleteSongCache: wrap(async (fileHash: string) => {
        await deleteSongCache(fileHash);
        markSongCacheDeleted(fileHash);
      }, invalidateSongs),
      deleteSongCacheAll: wrapBulk(
        BulkActionKind.Immediate,
        "Cache deleted",
        () => deleteSongCacheAll(currentFilters()),
        invalidateSongs,
      ),
      reanalyzeTranscript: wrap(reanalyzeTranscript, invalidateSongs),
      reanalyzeFull: wrap(reanalyzeFull, invalidateSongs),
      realign: wrap(realign, invalidateSongs),
      realignAll: wrapBulk(
        BulkActionKind.Queued,
        "realigning",
        () => realignAll(currentFilters()),
        invalidateSongs,
      ),
      reanalyzeForceTranscribe: wrap(reanalyzeForceTranscribe, invalidateSongs),
      refreshMetadata: wrapResult(refreshMetadata, invalidateSongs),
      refreshMetadataAll: wrapBulk(
        BulkActionKind.Immediate,
        "Refreshed metadata",
        () => refreshMetadataAll(currentFilters()),
        invalidateSongs,
      ),
      reanalyzeAllFull: wrapBulk(
        BulkActionKind.Queued,
        "full reanalysis",
        () => reanalyzeAllFull(currentFilters()),
        invalidateSongs,
      ),
      reanalyzeAllTranscript: wrapBulk(
        BulkActionKind.Queued,
        "refetching lyrics & aligning",
        (language?: string) => reanalyzeAllTranscript(currentFilters(), language),
        invalidateSongs,
      ),
      reanalyzeAllForceTranscribe: wrapBulk(
        BulkActionKind.Queued,
        "force transcribing",
        () => reanalyzeAllForceTranscribe(currentFilters()),
        invalidateSongs,
      ),
    };
  }, [queryClient, artist, album, playlist, query, status, transcript_source, search]);
};
