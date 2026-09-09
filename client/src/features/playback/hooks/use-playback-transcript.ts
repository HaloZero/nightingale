/**
 * Loads the transcript for the current track and normalizes segments for display.
 */

import { useEffect, useState } from 'react';

import { loadTranscript } from '@/bridge/playback';
import { splitLongSegments } from '@/features/playback/utils/transcript-segments';
import type { Segment, Transcript } from '@/types/Transcript';

export function usePlaybackTranscript(fileHash: string) {
  const [segments, setSegments] = useState<Segment[]>([]);
  const [transcriptSource, setTranscriptSource] = useState('generated');

  useEffect(() => {
    void loadTranscript(fileHash).then((transcript: Transcript) => {
      setSegments(splitLongSegments(transcript.segments));
      setTranscriptSource(transcript.source ?? 'generated');
      return undefined;
    });
  }, [fileHash]);

  return { segments, transcriptSource };
}
