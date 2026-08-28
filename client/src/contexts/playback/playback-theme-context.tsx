/**
 * Owns the visual background selection: shader / Pixabay / source-video index,
 * the current Pixabay flavor, and the (async) playable source-video path.
 *
 * Cycle handlers persist their result via the playback config, so call sites
 * just trigger `cycleTheme()` / `cycleFlavor()` without threading persistence.
 */

import {
  SOURCE_VIDEO_INDEX,
  YOUTUBE_INDEX,
  nextFlavorIndex,
  nextThemeIndex,
} from "@/components/playback/background";
import { FLAVORS, type VideoFlavor } from "@/lib/playback/video-flavor";
import { ensurePlayableSourceVideo, loadYoutubeBackground } from "@/bridge/playback";
import type { AppConfig } from "@/types/AppConfig";
import type { Song } from "@/types/Song";
import type { YoutubeBackground } from "@/types/YoutubeBackground";
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
} from "react";
import { usePlaybackConfigPersist } from "@/hooks/playback/use-playback-config-persist";

export interface PlaybackThemeState {
  themeIndex: number;
  flavorIndex: number;
  videoFlavor: VideoFlavor;
  sourceVideoPath: string | undefined;
  sourceVideoTempoRatio: number;
  hasSourceVideo: boolean;
  youtubeBackground: YoutubeBackground | null;
}

export interface PlaybackThemeActions {
  setThemeIndex: Dispatch<SetStateAction<number>>;
  setFlavorIndex: Dispatch<SetStateAction<number>>;
  cycleTheme: () => void;
  cycleFlavor: () => void;
}

const ThemeStateContext = createContext<PlaybackThemeState | null>(null);
const ThemeActionsContext = createContext<PlaybackThemeActions | null>(null);

interface PlaybackThemeProviderProps {
  song: Song;
  config: AppConfig | null;
  children: ReactNode;
}

export function PlaybackThemeProvider({ song, config, children }: PlaybackThemeProviderProps) {
  const fileHash = song.file_hash;
  const initialTheme = config?.last_theme ?? 0;
  const initialVideoFlavor = config?.last_video_flavor ?? 0;

  const [themeIndex, setThemeIndex] = useState(song.is_video ? SOURCE_VIDEO_INDEX : initialTheme);
  const [flavorIndex, setFlavorIndex] = useState(initialVideoFlavor);

  const persistConfig = usePlaybackConfigPersist(config);

  const [sourceVideoPath, setSourceVideoPath] = useState<string | undefined>(
    song.is_video ? song.path : undefined,
  );

  useEffect(() => {
    if (!song.is_video) {
      setSourceVideoPath(undefined);
      return;
    }

    let cancelled = false;
    setSourceVideoPath(song.path);

    void ensurePlayableSourceVideo(fileHash)
      .then((path) => {
        if (cancelled) return;
        if (path && path !== song.path) {
          setSourceVideoPath(path);
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [fileHash, song.is_video, song.path]);

  const [youtubeBackground, setYoutubeBackground] = useState<YoutubeBackground | null>(null);

  useEffect(() => {
    let cancelled = false;
    setYoutubeBackground(null);

    void loadYoutubeBackground(fileHash)
      .then((background) => {
        if (!cancelled) setYoutubeBackground(background);
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
    }),
    [themeIndex, flavorIndex, sourceVideoPath, song.tempo, song.is_video, youtubeBackground],
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
    throw new Error("usePlaybackThemeState must be used within a PlaybackThemeProvider");
  }
  return ctx;
}

export function usePlaybackThemeActions(): PlaybackThemeActions {
  const ctx = useContext(ThemeActionsContext);
  if (!ctx) {
    throw new Error("usePlaybackThemeActions must be used within a PlaybackThemeProvider");
  }
  return ctx;
}
