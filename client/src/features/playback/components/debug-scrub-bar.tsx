/**
 * Developer-only scrub bar for visually/aurally checking lyric-alignment
 * quality: drags/clicks go through the real `seek` transport action, so
 * audio, lyrics, and any synced video stay on the same timeline. Rendered
 * only when `AppConfig.debug_mode` is on (see `playback-inner.tsx`).
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import {
  usePlaybackTransportActions,
  usePlaybackTransportState,
} from '@/features/playback/providers';
import { Slider } from '@/shared/components/ui/slider';

/** Minimum time delta between live-position re-renders while not dragging; keeps
 * the rAF-driven subscription from forcing a React render on every frame. */
const POSITION_UPDATE_INTERVAL_SEC = 0.1;

function formatTime(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds) % 60;
  return `${mins}:${secs.toString().padStart(2, '0')}`;
}

export function DebugScrubBar() {
  const { duration } = usePlaybackTransportState();
  const { subscribe, getCurrentTime, seek } = usePlaybackTransportActions();

  const [position, setPosition] = useState(getCurrentTime);
  const draggingRef = useRef(false);
  const lastUpdateRef = useRef(0);

  useEffect(
    () =>
      subscribe((time) => {
        if (
          draggingRef.current ||
          Math.abs(time - lastUpdateRef.current) < POSITION_UPDATE_INTERVAL_SEC
        ) {
          return;
        }
        lastUpdateRef.current = time;
        setPosition(time);
      }),
    [subscribe],
  );

  const handleValueChange = useCallback(([value]: number[]) => {
    draggingRef.current = true;
    setPosition(value);
  }, []);

  const handleValueCommit = useCallback(
    ([value]: number[]) => {
      draggingRef.current = false;
      lastUpdateRef.current = value;
      seek(value);
    },
    [seek],
  );

  if (duration <= 0) {
    return null;
  }

  return (
    <div className="pointer-events-auto absolute inset-x-0 bottom-0 z-30 flex items-center gap-2 bg-black/40 px-4 py-2 md:px-6">
      <span className="font-mono text-xs text-amber-300">DEBUG</span>
      <Slider
        aria-label="Scrub playback position (debug mode)"
        min={0}
        max={duration}
        step={0.1}
        value={[position]}
        onValueChange={handleValueChange}
        onValueCommit={handleValueCommit}
        className="flex-1"
      />
      <span className="w-20 shrink-0 font-mono text-xs text-amber-300">
        {formatTime(position)} / {formatTime(duration)}
      </span>
    </div>
  );
}
