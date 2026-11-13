import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';

import type { VideoFlavor } from '@/features/playback/lib/video-flavor';
import { usePlaybackMicActions } from '@/features/playback/providers/playback-mic-context';
import { usePlaybackStartupActions } from '@/features/playback/providers/playback-startup-context';
import { usePlaybackThemeState } from '@/features/playback/providers/playback-theme-context';
import { usePlaybackTransportState } from '@/features/playback/providers/playback-transport-context';
import { t, useTranslation } from '@/i18n';
import { useIsMobileWeb } from '@/shared/hooks/use-is-mobile';

import { BackgroundLifecycleContext, useBackgroundLifecycle } from './background-lifecycle-context';
import { CustomBackground } from './custom-background';
import { PitchGuideBackground } from './pitch-guide-background';
import { PixabayVideo } from './pixabay-video';
import { ShaderVisualizer } from './shader-visualizer';
import { SourceVideo } from './source-video';
import { PITCH_GUIDE_SHADER_ID, themeKey, type PlaybackTheme } from './theme';

const LOAD_TIMEOUT_MS = 15_000;

function ShaderBranch({
  id,
  shaderIndex,
  isPlaying,
}: {
  id: string;
  shaderIndex: number;
  isPlaying: boolean;
}) {
  const { reactiveRef } = usePlaybackMicActions();
  const { ready } = useBackgroundLifecycle();
  if (id === PITCH_GUIDE_SHADER_ID) {
    return (
      <PitchGuideBackground
        shaderIndex={shaderIndex}
        isPlaying={isPlaying}
        reactiveRef={reactiveRef}
      />
    );
  }
  return (
    <ShaderVisualizer
      shaderIndex={shaderIndex}
      isPlaying={isPlaying}
      reactiveRef={reactiveRef}
      onReady={ready}
    />
  );
}

function CustomBranch({ isPlaying }: { isPlaying: boolean }) {
  const { theme } = usePlaybackThemeState();
  const { reactiveRef } = usePlaybackMicActions();
  if (theme.kind !== 'custom') {
    return null;
  }
  return (
    <CustomBackground
      key={theme.background.id}
      background={theme.background}
      isPlaying={isPlaying}
      reactiveRef={reactiveRef}
    />
  );
}

function FallbackBackground({ isPlaying }: { isPlaying: boolean }) {
  const { reactiveRef } = usePlaybackMicActions();
  const { ready } = useBackgroundLifecycle();

  return (
    <ShaderVisualizer
      shaderIndex={0}
      isPlaying={isPlaying}
      reactiveRef={reactiveRef}
      onReady={ready}
    />
  );
}

function DecorativeBackground({
  theme,
  videoFlavor,
  sourceVideoPath,
  isPlaying,
  fallback,
}: {
  theme: PlaybackTheme;
  videoFlavor: VideoFlavor;
  sourceVideoPath: string | undefined;
  isPlaying: boolean;
  fallback: boolean;
}) {
  if (fallback) {
    return <FallbackBackground isPlaying={isPlaying} />;
  }
  return (
    <>
      {typeof sourceVideoPath === 'string' && sourceVideoPath !== '' && (
        <SourceVideo isActive={theme.kind === 'source'} />
      )}
      {theme.kind === 'shader' && (
        <ShaderBranch id={theme.id} shaderIndex={theme.shaderIndex} isPlaying={isPlaying} />
      )}
      {theme.kind === 'pixabay' && <PixabayVideo flavor={videoFlavor} isPlaying={isPlaying} />}
      {theme.kind === 'custom' && <CustomBranch isPlaying={isPlaying} />}
    </>
  );
}

function ActiveBackground() {
  const { isPlaying } = usePlaybackTransportState();
  const { theme, videoFlavor, sourceVideoPath } = usePlaybackThemeState();
  const { markBackgroundReady } = usePlaybackStartupActions();
  const isMobileWeb = useIsMobileWeb();
  const activeThemeKey = themeKey(theme);
  const readyRef = useRef(false);
  const failedRef = useRef(false);
  const [settled, setSettled] = useState(false);
  const [fallback, setFallback] = useState(false);

  const ready = useCallback(() => {
    if (readyRef.current || (failedRef.current && !fallback)) {
      return;
    }
    readyRef.current = true;
    setSettled(true);
    markBackgroundReady(activeThemeKey);
  }, [activeThemeKey, fallback, markBackgroundReady]);

  const failed = useCallback((message: string) => {
    if (failedRef.current || readyRef.current) {
      return;
    }
    failedRef.current = true;
    toast.error(t('playback.background.backgroundError', { value1: message }));
    setFallback(true);
  }, []);

  useEffect(() => {
    if (settled || fallback) {
      return undefined;
    }
    const timeout = window.setTimeout(
      () => failed(t('playback.background.timeout')),
      LOAD_TIMEOUT_MS,
    );
    return () => window.clearTimeout(timeout);
  }, [failed, fallback, settled]);

  // Shaders, Pixabay clips, source video, and custom backgrounds are all
  // decorative on top of lyrics/audio -- skip mounting any of them on
  // mobile web so no background bytes compete with the audio/lyrics fetch
  // on a constrained connection. Still report ready immediately so startup
  // isn't stuck waiting for a background that will never mount.
  useEffect(() => {
    if (isMobileWeb) {
      ready();
    }
  }, [isMobileWeb, ready]);

  const lifecycle = useMemo(() => ({ ready, failed }), [failed, ready]);

  return (
    <BackgroundLifecycleContext.Provider value={lifecycle}>
      <div className="fixed inset-0">
        {!isMobileWeb && (
          <DecorativeBackground
            theme={theme}
            videoFlavor={videoFlavor}
            sourceVideoPath={sourceVideoPath}
            isPlaying={isPlaying}
            fallback={fallback}
          />
        )}
      </div>
    </BackgroundLifecycleContext.Provider>
  );
}

function BackgroundImpl() {
  useTranslation();
  const { theme } = usePlaybackThemeState();
  return <ActiveBackground key={themeKey(theme)} />;
}

export const Background = memo(BackgroundImpl);
