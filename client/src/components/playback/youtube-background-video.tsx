import { playbackAdapter } from "@/bridge/playback";
import {
  usePlaybackThemeState,
  usePlaybackTransportActions,
  usePlaybackTransportState,
} from "@/contexts/playback";
import { useSourceVideoSync } from "@/hooks/use-source-video-sync";
import { useEffect, useRef, useState } from "react";
import { VIDEO_CLASS_NAME } from "@/lib/playback/video-styles";

interface YoutubeBackgroundVideoProps {
  isActive: boolean;
}

function useMediaUrl(filePath: string): string | null {
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setSrc(null);

    void playbackAdapter.init().then(() => {
      if (cancelled) return;
      setSrc(playbackAdapter.toMediaUrl(filePath));
    });

    return () => {
      cancelled = true;
    };
  }, [filePath]);

  return src;
}

/** Mirrors `source-video.tsx` exactly, sourcing the video + sync offset
 * from `youtubeBackground` (fetched by `PlaybackThemeProvider`) instead of
 * the song's own bundled video. `tempoRatio` stays at 1 -- the official
 * YouTube video isn't tempo-shifted the way a user's own bundled video
 * might be, only offset by a constant (`video_sync::SyncResult`). */
export const YoutubeBackgroundVideo = ({ isActive }: YoutubeBackgroundVideoProps) => {
  const { youtubeBackground } = usePlaybackThemeState();
  const { isReady, isPlaying } = usePlaybackTransportState();
  const { subscribe, getCurrentTime } = usePlaybackTransportActions();

  const videoRef = useRef<HTMLVideoElement>(null);
  const src = useMediaUrl(youtubeBackground?.video_asset_path ?? "");

  const playWhenActive = isReady && isPlaying && isActive;

  const { ready } = useSourceVideoSync({
    videoRef,
    src: youtubeBackground ? src : null,
    isPlaying: playWhenActive,
    tempoRatio: 1,
    offsetSecs: youtubeBackground?.offset_secs ?? 0,
    subscribe,
    getCurrentTime,
  });

  if (!youtubeBackground || !src) return null;

  return (
    <video
      ref={videoRef}
      className={VIDEO_CLASS_NAME}
      style={{ visibility: ready && isActive ? "visible" : "hidden" }}
      src={src}
      muted
      playsInline
    />
  );
};
