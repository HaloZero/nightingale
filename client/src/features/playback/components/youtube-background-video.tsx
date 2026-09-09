import { useEffect, useRef, useState } from 'react';

import { playbackAdapter } from '@/bridge/playback';
import { useSourceVideoSync } from '@/features/playback/hooks/use-source-video-sync';
import { VIDEO_CLASS_NAME } from '@/features/playback/lib/video-styles';
import { usePlaybackThemeState } from '@/features/playback/providers/playback-theme-context';
import {
  usePlaybackTransportActions,
  usePlaybackTransportState,
} from '@/features/playback/providers/playback-transport-context';

type YoutubeBackgroundVideoProps = {
  isActive: boolean;
};

// Keyed by the `filePath` a fetch was started for, so a `filePath` change
// mid-flight doesn't let a stale response land as `src` -- render derives
// staleness from this pair instead of the effect resetting `src` to `null`
// synchronously on every change.
type MediaUrlState = { filePath: string; src: string | null };

function useMediaUrl(filePath: string): string | null {
  const [state, setState] = useState<MediaUrlState>({ filePath, src: null });

  useEffect(() => {
    let cancelled = false;

    void playbackAdapter.init().then(() => {
      if (cancelled) {
        return undefined;
      }
      setState({ filePath, src: playbackAdapter.toMediaUrl(filePath) });
      return undefined;
    });

    return () => {
      cancelled = true;
    };
  }, [filePath]);

  return state.filePath === filePath ? state.src : null;
}

function useYoutubeBackgroundVideo(isActive: boolean) {
  const { youtubeBackground } = usePlaybackThemeState();
  const { isReady, isPlaying } = usePlaybackTransportState();
  const { subscribe, getCurrentTime } = usePlaybackTransportActions();

  const videoRef = useRef<HTMLVideoElement>(null);
  const src = useMediaUrl(youtubeBackground?.video_asset_path ?? '');
  const playWhenActive = isReady && isPlaying && isActive;

  const { ready } = useSourceVideoSync({
    videoRef,
    src: youtubeBackground !== null ? src : null,
    isPlaying: playWhenActive,
    tempoRatio: 1,
    offsetSecs: youtubeBackground?.offset_secs ?? 0,
    subscribe,
    getCurrentTime,
  });

  return { videoRef, src, ready, hasSource: youtubeBackground !== null && src !== null };
}

/** Mirrors `source-video.tsx` exactly, sourcing the video + sync offset
 * from `youtubeBackground` (fetched by `PlaybackThemeProvider`) instead of
 * the song's own bundled video. `tempoRatio` stays at 1 -- the official
 * YouTube video isn't tempo-shifted the way a user's own bundled video
 * might be, only offset by a constant (`video_sync::SyncResult`). */
export const YoutubeBackgroundVideo = ({ isActive }: YoutubeBackgroundVideoProps) => {
  const { videoRef, src, ready, hasSource } = useYoutubeBackgroundVideo(isActive);

  if (!hasSource) {
    return null;
  }

  return (
    <video
      ref={videoRef}
      className={VIDEO_CLASS_NAME}
      style={{ visibility: ready && isActive ? 'visible' : 'hidden' }}
      src={src ?? undefined}
      muted
      playsInline
    />
  );
};
