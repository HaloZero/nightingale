/**
 * Drives the end-of-song result dialog: watches transport.isFinished + the
 * skip-outro pending flag, persists the run's score to the active profile,
 * plays the success chime, and exposes the props the result dialog needs.
 */

import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';

import successSoundUrl from '@/assets/sounds/success.mp3';
import { addScore } from '@/bridge/profile';
import {
  usePlaybackQueueQuery,
  useStartNextPlaybackQueueSong,
} from '@/features/playback-queue/use-playback-queue';
import {
  usePlaybackMicState,
  usePlaybackTranscriptActions,
  usePlaybackTranscriptState,
  usePlaybackTransportActions,
  usePlaybackTransportState,
} from '@/features/playback/providers';
import { useProfiles } from '@/features/profiles/queries/use-profiles';
import { useLatestRef } from '@/shared/hooks/use-latest-ref';
import { PROFILES } from '@/shared/query-keys';
import type { ScoreRecord } from '@/types/ScoreRecord';
import type { Song } from '@/types/Song';

export type PlaybackResult = {
  open: boolean;
  score: number;
  scores: ScoreRecord[];
  activeProfile: string | null;
  nextPending: boolean;
  onBack: () => void;
  onNext?: () => void;
};

export function usePlaybackResult(song: Song, queuePlayback: boolean): PlaybackResult {
  const fileHash = song.file_hash;
  const queryClient = useQueryClient();
  const { data: profileData, isLoading: profilesLoading } = useProfiles();
  const { data: entries = [] } = usePlaybackQueueQuery();
  const { isPreparing, playNext } = useStartNextPlaybackQueueSong(entries);

  const { isFinished } = usePlaybackTransportState();
  const { handleExit } = usePlaybackTransportActions();
  const { rawScore } = usePlaybackMicState();
  const { skipOutroPending } = usePlaybackTranscriptState();
  const { clearSkipOutroPending } = usePlaybackTranscriptActions();

  const [showResult, setShowResult] = useState(false);
  const [resultScore, setResultScore] = useState(0);

  const scoreRef = useLatestRef(rawScore);
  const finishHandledRef = useRef(false);

  useEffect(() => {
    if (!isFinished && !skipOutroPending) {
      return;
    }

    if (profilesLoading) {
      return;
    }

    if (finishHandledRef.current) {
      return;
    }

    finishHandledRef.current = true;
    clearSkipOutroPending();

    const finalScore = scoreRef.current;
    const active = profileData?.active ?? null;
    const shouldShowResult = queuePlayback || finalScore > 0;

    if (!shouldShowResult) {
      handleExit();
      return;
    }

    void (async () => {
      try {
        if (active !== null) {
          await addScore(fileHash, finalScore);
          await queryClient.invalidateQueries({ queryKey: PROFILES });
        }
      } catch (e) {
        toast.error(`Could not save score: ${e instanceof Error ? e.message : String(e)}`);
      }
      setResultScore(finalScore);
      setShowResult(true);
    })();
  }, [
    isFinished,
    skipOutroPending,
    fileHash,
    handleExit,
    profileData,
    profilesLoading,
    queryClient,
    clearSkipOutroPending,
    scoreRef,
    queuePlayback,
  ]);

  useEffect(() => {
    if (!showResult) {
      return undefined;
    }

    const audioEl = new Audio(successSoundUrl);
    void audioEl.play().catch(() => {});

    return () => {
      audioEl.pause();
      audioEl.src = '';
    };
  }, [showResult]);

  const onBack = useCallback(() => {
    setShowResult(false);
    handleExit();
  }, [handleExit]);
  const onNext = useCallback(() => playNext(), [playNext]);
  const hasNext = queuePlayback && entries.length > 0;

  return {
    open: showResult,
    score: resultScore,
    scores: profileData?.scores ?? [],
    activeProfile: profileData?.active ?? null,
    nextPending: isPreparing,
    onBack,
    onNext: hasNext ? onNext : undefined,
  };
}
