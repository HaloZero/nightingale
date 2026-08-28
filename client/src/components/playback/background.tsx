import {
  usePlaybackMicActions,
  usePlaybackThemeState,
  usePlaybackTransportState,
} from "@/contexts/playback";
import { FLAVORS, type VideoFlavor } from "@/lib/playback/video-flavor";
import { memo } from "react";
import { PixabayVideo } from "./pixabay-video";
import { ShaderVisualizer } from "./shader-visualizer";
import { loadingFragment, shaders } from "./shaders";
import { SourceVideo } from "./source-video";
import { YoutubeBackgroundVideo } from "./youtube-background-video";

export type ThemeMode = "shader" | "pixabay" | "source" | "youtube";

const SHADER_COUNT = shaders.length;
const PIXABAY_INDEX = SHADER_COUNT;
export const SOURCE_VIDEO_INDEX = SHADER_COUNT + 1;
export const YOUTUBE_INDEX = SHADER_COUNT + 2;

export function themeMode(index: number): ThemeMode {
  if (index === PIXABAY_INDEX) {
    return "pixabay";
  }

  if (index === SOURCE_VIDEO_INDEX) {
    return "source";
  }

  if (index === YOUTUBE_INDEX) {
    return "youtube";
  }

  return "shader";
}

export function themeName(index: number, videoFlavor: VideoFlavor): string {
  const mode = themeMode(index);

  if (mode === "source") {
    return "Source Video";
  }

  if (mode === "youtube") {
    return "YouTube Video";
  }

  if (mode === "pixabay") {
    const name = videoFlavor.charAt(0).toUpperCase() + videoFlavor.slice(1);

    return `Video — ${name}`;
  }

  return shaders[index % SHADER_COUNT].name;
}

/** Shaders + Pixabay are always available; source-video and YouTube-video
 * are appended in this fixed relative order only when available, so
 * `SOURCE_VIDEO_INDEX`/`YOUTUBE_INDEX` stay meaningful fixed constants
 * (importable/comparable elsewhere) regardless of which combination of the
 * two is present for a given song -- cycling walks this list rather than
 * doing index arithmetic that would otherwise need to reshuffle when only
 * one of the two extra slots is available. */
function availableThemeIndices(hasSourceVideo: boolean, hasYoutubeBackground: boolean): number[] {
  const base = Array.from({ length: PIXABAY_INDEX + 1 }, (_, i) => i);
  if (hasSourceVideo) base.push(SOURCE_VIDEO_INDEX);
  if (hasYoutubeBackground) base.push(YOUTUBE_INDEX);
  return base;
}

export function themeCount(hasSourceVideo: boolean, hasYoutubeBackground: boolean): number {
  return availableThemeIndices(hasSourceVideo, hasYoutubeBackground).length;
}

export function nextThemeIndex(
  current: number,
  hasSourceVideo: boolean,
  hasYoutubeBackground: boolean,
): number {
  const list = availableThemeIndices(hasSourceVideo, hasYoutubeBackground);
  const pos = list.indexOf(current);
  return list[pos === -1 ? 0 : (pos + 1) % list.length];
}

export function nextFlavorIndex(current: number): number {
  return (current + 1) % FLAVORS.length;
}

export function isPixabayTheme(index: number): boolean {
  return index === PIXABAY_INDEX;
}

function ShaderBranch({ themeIndex, isPlaying }: { themeIndex: number; isPlaying: boolean }) {
  const { reactiveRef } = usePlaybackMicActions();
  return (
    <ShaderVisualizer
      shaderIndex={themeIndex % SHADER_COUNT}
      isPlaying={isPlaying}
      reactiveRef={reactiveRef}
    />
  );
}

function BackgroundImpl() {
  const { isReady, isPlaying } = usePlaybackTransportState();
  const { themeIndex, videoFlavor, sourceVideoPath, youtubeBackground } = usePlaybackThemeState();

  if (!isReady) {
    return (
      <div className="fixed inset-0">
        <ShaderVisualizer shaderIndex={0} isPlaying={true} customFragment={loadingFragment} />
      </div>
    );
  }

  const mode = themeMode(themeIndex);
  const showSourceVideo = mode === "source";
  const showYoutubeBackground = mode === "youtube";
  const playing = isReady && isPlaying;

  return (
    <div className="fixed inset-0">
      {sourceVideoPath && <SourceVideo isActive={showSourceVideo} />}
      {youtubeBackground && <YoutubeBackgroundVideo isActive={showYoutubeBackground} />}
      {mode === "shader" && <ShaderBranch themeIndex={themeIndex} isPlaying={playing} />}
      {mode === "pixabay" && <PixabayVideo flavor={videoFlavor} isPlaying={playing} />}
    </div>
  );
}

export const Background = memo(BackgroundImpl);
