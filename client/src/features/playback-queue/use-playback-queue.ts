import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect } from 'react';
import { toast } from 'sonner';

import {
  addPlaybackQueueEntry,
  clearPlaybackQueue,
  loadPlaybackQueue,
  onPlaybackQueueChanged,
  removePlaybackQueueEntry,
  type PlaybackQueueEntry,
} from '@/bridge/playback-queue';
import type { PlaybackTarget } from '@/bridge/playback-session';
import { usePlaybackLauncher } from '@/features/playback/hooks/use-playback-launcher';
import {
  preparePlayback,
  type PreparePlaybackInput,
} from '@/features/playback/mutations/use-prepare-playback-mutation';
import { PLAYBACK_QUEUE } from '@/shared/query-keys';
import type { Song } from '@/types/Song';

export function usePlaybackQueueQuery() {
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: PLAYBACK_QUEUE, queryFn: loadPlaybackQueue });

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onPlaybackQueueChanged((entries) => {
      queryClient.setQueryData(PLAYBACK_QUEUE, entries);
    }).then((stop) => {
      if (cancelled) {
        stop();
        return undefined;
      }
      unlisten = stop;
      void queryClient.invalidateQueries({ queryKey: PLAYBACK_QUEUE });
      return undefined;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient]);

  return query;
}

export function useAddPlaybackQueueEntry() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ song, tempo, keyOffset }: PreparePlaybackInput) =>
      addPlaybackQueueEntry(song.file_hash, tempo, keyOffset),
    onSuccess: (entries) => queryClient.setQueryData(PLAYBACK_QUEUE, entries),
    onError: (error: Error) => toast.error(`Could not add song to queue: ${error.message}`),
  });
}

export function useRemovePlaybackQueueEntry() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: removePlaybackQueueEntry,
    onSuccess: (entries) => queryClient.setQueryData(PLAYBACK_QUEUE, entries),
    onError: (error: Error) => toast.error(`Could not remove song from queue: ${error.message}`),
  });
}

export function useClearPlaybackQueue() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: clearPlaybackQueue,
    onSuccess: (entries) => queryClient.setQueryData(PLAYBACK_QUEUE, entries),
    onError: (error: Error) => toast.error(`Could not clear queue: ${error.message}`),
  });
}

type StartInput = {
  entry: PlaybackQueueEntry;
  target: PlaybackTarget;
};

type StartResult = {
  id: string;
  song: Song;
  entries: PlaybackQueueEntry[];
  target: PlaybackTarget;
};

export function useStartNextPlaybackQueueSong(entries: PlaybackQueueEntry[]) {
  const queryClient = useQueryClient();
  const { launch, reserveTarget } = usePlaybackLauncher();
  const { mutate, isLoading } = useMutation({
    mutationFn: async ({ entry, target }: StartInput): Promise<StartResult> => {
      const song = await preparePlayback({
        song: entry.song,
        tempo: entry.tempo,
        keyOffset: entry.keyOffset,
      });
      const nextEntries = await removePlaybackQueueEntry(entry.id);
      return { id: entry.id, song, entries: nextEntries, target };
    },
    onSuccess: ({ id, song, entries: nextEntries, target }) => {
      queryClient.setQueryData(PLAYBACK_QUEUE, nextEntries);
      void launch({ song, queuePlayback: true, playbackId: id }, target);
    },
    onError: (error: Error, { target }) => {
      target?.close();
      toast.error(`Could not start queued song: ${error.message}`);
    },
  });

  const playNext = useCallback(() => {
    if (entries.length === 0 || isLoading) {
      return;
    }
    const target = reserveTarget();
    if (target === undefined) {
      return;
    }
    mutate({ entry: entries[0], target });
  }, [entries, isLoading, mutate, reserveTarget]);

  return { playNext, isPreparing: isLoading };
}
