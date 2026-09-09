/**
 * Owns the visual background selection: shader / Pixabay / source-video index,
 * the current Pixabay flavor, and the (async) playable source-video path.
 *
 * Cycle handlers persist their result via the playback config, so call sites
 * just trigger `cycleTheme()` / `cycleFlavor()` without threading persistence.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
} from 'react';

import { ensurePlayableSourceVideo, loadYoutubeBackground } from '@/bridge/playback';
import {
  SOURCE_VIDEO_INDEX,
  YOUTUBE_INDEX,
  nextFlavorIndex,
  nextThemeIndex,
} from '@/features/playback/components/theme';
import { usePlaybackConfigPersist } from '@/features/playback/hooks/use-playback-config-persist';
import { FLAVORS, type VideoFlavor } from '@/features/playback/lib/video-flavor';
import type { AppConfig } from '@/types/AppConfig';
import type { Song } from '@/types/Song';
import type { YoutubeBackground } from '@/types/YoutubeBackground';

export type PlaybackThemeState = {
  themeIndex: number;
  flavorIndex: number;
  videoFlavor: VideoFlavor;
  sourceVideoPath: string | undefined;
  sourceVideoTempoRatio: number;
  hasSourceVideo: boolean;
  youtubeBackground: YoutubeBackground | null;
  pixabayRotationEnabled: boolean;
};

export type PlaybackThemeActions = {
  setThemeIndex: Dispatch<SetStateAction<number>>;
  setFlavorIndex: Dispatch<SetStateAction<number>>;
  cycleTheme: () => void;
  cycleFlavor: () => void;
};

const ThemeStateContext = createContext<PlaybackThemeState | null>(null);
const ThemeActionsContext = createContext<PlaybackThemeActions | null>(null);

type PlaybackThemeProviderProps = {
  song: Song;
  config: AppConfig | null;
  children: ReactNode;
};

type PlayableVideo = { fileHash: string; path: string };

function resolveSourceVideoPath(
  song: Song,
  playableVideo: PlayableVideo | null,
): string | undefined {
  if (!song.is_video) {
    return undefined;
  }

  return playableVideo?.fileHash === song.file_hash ? playableVideo.path : song.path;
}

export function PlaybackThemeProvider({ song, config, children }: PlaybackThemeProviderProps) {
  const fileHash = song.file_hash;
  const initialTheme = config?.last_theme ?? 0;
  const initialVideoFlavor = config?.last_video_flavor ?? 0;

  const [themeIndex, setThemeIndex] = useState(song.is_video ? SOURCE_VIDEO_INDEX : initialTheme);
  const [flavorIndex, setFlavorIndex] = useState(initialVideoFlavor);

  const persistConfig = usePlaybackConfigPersist(config);

  const [playableVideo, setPlayableVideo] = useState<PlayableVideo | null>(null);

  useEffect(() => {
    if (!song.is_video) {
      return undefined;
    }

    let cancelled = false;

    void ensurePlayableSourceVideo(fileHash)
      .then((path) => {
        if (!cancelled && typeof path === 'string' && path !== '') {
          setPlayableVideo({ fileHash, path });
        }
        return undefined;
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [fileHash, song.is_video]);

  const sourceVideoPath = resolveSourceVideoPath(song, playableVideo);

  // Keyed by the `fileHash` a fetch was started for, so a song change
  // mid-flight doesn't let a stale response land as the current
  // `youtubeBackground` -- render derives staleness from this pair instead
  // of the effect resetting state to `null` synchronously on every change.
  const [youtubeBackgroundState, setYoutubeBackgroundState] = useState<{
    fileHash: string;
    background: YoutubeBackground | null;
  }>({ fileHash, background: null });
  const youtubeBackground =
    youtubeBackgroundState.fileHash === fileHash ? youtubeBackgroundState.background : null;

  useEffect(() => {
    let cancelled = false;

    void loadYoutubeBackground(fileHash)
      .then((background) => {
        if (!cancelled) {
          setYoutubeBackgroundState({ fileHash, background });
        }
        return undefined;
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [fileHash]);

  const cycleTheme = useCallback(() => {
    setThemeIndex((prev) => {
      const next = nextThemeIndex(prev, song.is_video, youtubeBackground !== null);
      if (next !== SOURCE_VIDEO_INDEX && next !== YOUTUBE_INDEX) {
        persistConfig({ last_theme: next });
      }
      return next;
    });
  }, [song.is_video, youtubeBackground, persistConfig]);

  const cycleFlavor = useCallback(() => {
    setFlavorIndex((prev) => {
      const next = nextFlavorIndex(prev);
      persistConfig({ last_video_flavor: next });
      return next;
    });
  }, [persistConfig]);

  const stateValue = useMemo<PlaybackThemeState>(
    () => ({
      themeIndex,
      flavorIndex,
      videoFlavor: FLAVORS[flavorIndex % FLAVORS.length],
      sourceVideoPath,
      sourceVideoTempoRatio: song.tempo,
      hasSourceVideo: song.is_video,
      youtubeBackground,
      pixabayRotationEnabled: config?.pixabay_video_rotation ?? false,
    }),
    [
      themeIndex,
      flavorIndex,
      sourceVideoPath,
      song.tempo,
      song.is_video,
      youtubeBackground,
      config?.pixabay_video_rotation,
    ],
  );

  const actionsValue = useMemo<PlaybackThemeActions>(
    () => ({
      setThemeIndex,
      setFlavorIndex,
      cycleTheme,
      cycleFlavor,
    }),
    [cycleTheme, cycleFlavor],
  );

  return (
    <ThemeStateContext.Provider value={stateValue}>
      <ThemeActionsContext.Provider value={actionsValue}>{children}</ThemeActionsContext.Provider>
    </ThemeStateContext.Provider>
  );
}

export function usePlaybackThemeState(): PlaybackThemeState {
  const ctx = useContext(ThemeStateContext);
  if (!ctx) {
    throw new Error('usePlaybackThemeState must be used within a PlaybackThemeProvider');
  }
  return ctx;
}

export function usePlaybackThemeActions(): PlaybackThemeActions {
  const ctx = useContext(ThemeActionsContext);
  if (!ctx) {
    throw new Error('usePlaybackThemeActions must be used within a PlaybackThemeProvider');
  }
  return ctx;
}
