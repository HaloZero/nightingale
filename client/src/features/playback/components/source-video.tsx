import { useEffect, useRef, useState } from 'react';

import { playbackAdapter } from '@/bridge/playback';
import { useSourceVideoSync } from '@/features/playback/hooks/use-source-video-sync';
import { VIDEO_CLASS_NAME } from '@/features/playback/lib/video-styles';
import { usePlaybackThemeState } from '@/features/playback/providers/playback-theme-context';
import {
  usePlaybackTransportActions,
  usePlaybackTransportState,
} from '@/features/playback/providers/playback-transport-context';

type SourceVideoProps = {
  isActive: boolean;
};

const playableSource = (path: string | undefined, src: string | null): src is string =>
  typeof path === 'string' && path !== '' && typeof src === 'string' && src !== '';

function useMediaUrl(filePath: string): string | null {
  const [media, setMedia] = useState<{ filePath: string; src: string } | null>(null);

  useEffect(() => {
    let cancelled = false;

    void playbackAdapter.init().then(() => {
      if (!cancelled) {
        setMedia({ filePath, src: playbackAdapter.toMediaUrl(filePath) });
      }
      return undefined;
    });

    return () => {
      cancelled = true;
    };
  }, [filePath]);

  return media?.filePath === filePath ? media.src : null;
}

export const SourceVideo = ({ isActive }: SourceVideoProps) => {
  const { sourceVideoPath, sourceVideoTempoRatio } = usePlaybackThemeState();
  const { isReady, isPlaying } = usePlaybackTransportState();
  const { subscribe, getCurrentTime } = usePlaybackTransportActions();

  const videoRef = useRef<HTMLVideoElement>(null);
  const src = useMediaUrl(sourceVideoPath ?? '');

  const playWhenActive = isReady && isPlaying && isActive;

  const { ready } = useSourceVideoSync({
    videoRef,
    src: playableSource(sourceVideoPath, src) ? src : null,
    isPlaying: playWhenActive,
    tempoRatio: sourceVideoTempoRatio,
    subscribe,
    getCurrentTime,
  });

  if (!playableSource(sourceVideoPath, src)) {
    return null;
  }

  return (
    <video
      ref={videoRef}
      className={VIDEO_CLASS_NAME}
      style={{ visibility: ready && isActive ? 'visible' : 'hidden' }}
      src={src}
      muted
      playsInline
    />
  );
};
