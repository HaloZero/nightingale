import type { TimeSubscriber } from "@/hooks/use-audio-player";
import { useLatestRef } from "@/hooks/use-latest-ref";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";

const DRIFT_LARGE_SEC = 0.75;
const DRIFT_CORRECT_SEC = 0.5;
const DRIFT_THROTTLE_MS = 500;

const PLAYBACK_RATE_MIN = 0.25;
const PLAYBACK_RATE_MAX = 4;

const INITIAL_SEEK_SKIP_THRESHOLD_SEC = 0.1;
const INITIAL_SEEK_TOLERANCE_SEC = 0.12;
const INITIAL_SEEK_END_PADDING_SEC = 0.05;
const INITIAL_SEEK_WATCHDOG_INTERVAL_MS = 120;

interface UseSourceVideoSyncOptions {
  videoRef: RefObject<HTMLVideoElement | null>;
  src: string | null;
  isPlaying: boolean;
  tempoRatio: number;
  subscribe: (fn: TimeSubscriber) => () => void;
  getCurrentTime: () => number;
  /** Constant seconds added after the tempo ratio -- e.g. a YouTube
   * background's own video timeline starting `offsetSecs` before the song's
   * own t=0 (see `video_sync::SyncResult.video_offset_secs`). Defaults to 0,
   * the source-video-file behavior this hook originally shipped with. */
  offsetSecs?: number;
}

function safePlay(video: HTMLVideoElement): void {
  void video.play().catch(() => {});
}

function normalizeRatio(ratio: number): number {
  if (!Number.isFinite(ratio) || ratio <= 0) return 1;

  return ratio;
}

function enforceSilent(video: HTMLVideoElement): void {
  // Some browsers ignore one or two of these in isolation; setting all three
  // is the only reliable way to keep the source video silent.
  video.defaultMuted = true;
  video.muted = true;
  video.volume = 0;
}

function applyPlaybackRate(video: HTMLVideoElement, ratio: number): void {
  video.playbackRate = Math.min(PLAYBACK_RATE_MAX, Math.max(PLAYBACK_RATE_MIN, ratio));
}

function correctDrift(
  video: HTMLVideoElement,
  target: number,
  lastSyncRef: { current: number },
  mode: "throttled" | "aggressive",
): void {
  const drift = Math.abs(video.currentTime - target);
  if (drift <= DRIFT_CORRECT_SEC) return;

  const now = performance.now();
  const shouldSnap =
    mode === "aggressive" ||
    drift > DRIFT_LARGE_SEC ||
    now - lastSyncRef.current > DRIFT_THROTTLE_MS;

  if (shouldSnap) {
    video.currentTime = target;
    lastSyncRef.current = now;
  }
}

function clampSeekTarget(target: number, video: HTMLVideoElement): number {
  if (Number.isFinite(video.duration) && video.duration > 0) {
    return Math.min(target, Math.max(0, video.duration - INITIAL_SEEK_END_PADDING_SEC));
  }

  return target;
}

/**
 * Drives the initial seek for the source video. Some WebKit builds miss the
 * `seeked` event when sources swap rapidly; this watchdog re-issues the seek
 * until the frame position falls within tolerance, then calls `onReady` once.
 */
function runInitialSeek(
  video: HTMLVideoElement,
  computeTarget: () => number,
  onReady: () => void,
): () => void {
  let finalized = false;
  let cleanup: (() => void) | undefined;

  const finalize = () => {
    if (finalized) return;
    finalized = true;
    onReady();
  };

  const start = () => {
    const target = computeTarget();

    if (target <= INITIAL_SEEK_SKIP_THRESHOLD_SEC) {
      finalize();
      return;
    }

    const seekTarget = clampSeekTarget(target, video);
    const isAligned = () => Math.abs(video.currentTime - seekTarget) <= INITIAL_SEEK_TOLERANCE_SEC;
    const tryFinalize = () => {
      if (isAligned()) finalize();
    };

    const watchdog = window.setInterval(() => {
      if (finalized) return;

      if (isAligned()) {
        finalize();
        return;
      }

      if (!video.seeking && video.readyState >= 1) {
        video.currentTime = seekTarget;
      }
    }, INITIAL_SEEK_WATCHDOG_INTERVAL_MS);

    video.addEventListener("seeked", tryFinalize);
    video.addEventListener("canplay", tryFinalize);
    video.currentTime = seekTarget;

    cleanup = () => {
      window.clearInterval(watchdog);
      video.removeEventListener("seeked", tryFinalize);
      video.removeEventListener("canplay", tryFinalize);
    };
  };

  if (video.readyState >= 1) {
    start();
  } else {
    const onMeta = () => start();
    video.addEventListener("loadedmetadata", onMeta, { once: true });
    cleanup = () => video.removeEventListener("loadedmetadata", onMeta);
  }

  return () => {
    finalized = true;
    cleanup?.();
  };
}

export function useSourceVideoSync({
  videoRef,
  src,
  isPlaying,
  tempoRatio,
  subscribe,
  getCurrentTime,
  offsetSecs = 0,
}: UseSourceVideoSyncOptions): { ready: boolean } {
  const [ready, setReady] = useState(false);
  const readyRef = useRef(false);
  const lastSyncRef = useRef(0);

  const tempoRatioRef = useLatestRef(tempoRatio);
  const isPlayingRef = useLatestRef(isPlaying);
  const offsetSecsRef = useLatestRef(offsetSecs);

  const currentRatio = useCallback(() => normalizeRatio(tempoRatioRef.current), [tempoRatioRef]);
  const toSourceTime = useCallback(
    (audioTime: number) => Math.max(0, audioTime * currentRatio() + offsetSecsRef.current),
    [currentRatio, offsetSecsRef],
  );

  useEffect(() => {
    const video = videoRef.current;
    if (!video || !src) return;

    readyRef.current = false;
    setReady(false);
    enforceSilent(video);
    applyPlaybackRate(video, currentRatio());

    return runInitialSeek(
      video,
      () => toSourceTime(getCurrentTime()),
      () => {
        readyRef.current = true;
        setReady(true);
        applyPlaybackRate(video, currentRatio());
        if (isPlayingRef.current) safePlay(video);
      },
    );
  }, [currentRatio, getCurrentTime, isPlayingRef, src, toSourceTime, videoRef]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video || !readyRef.current) return;

    enforceSilent(video);
    applyPlaybackRate(video, currentRatio());

    if (isPlaying) safePlay(video);
    else video.pause();
  }, [currentRatio, isPlaying, videoRef]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    applyPlaybackRate(video, currentRatio());
    if (!readyRef.current) return;

    correctDrift(video, toSourceTime(getCurrentTime()), lastSyncRef, "aggressive");
  }, [currentRatio, getCurrentTime, tempoRatio, toSourceTime, videoRef]);

  useEffect(() => {
    return subscribe((time) => {
      const video = videoRef.current;
      if (!video || !readyRef.current) return;

      correctDrift(video, toSourceTime(time), lastSyncRef, "throttled");
    });
  }, [subscribe, toSourceTime, videoRef]);

  return { ready };
}
