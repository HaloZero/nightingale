export const DEFAULT_PLAYBACK_SCALE = 1;
export const PLAYBACK_SCALE_MIN = 0.5;
export const PLAYBACK_SCALE_MAX = 2.5;

export const clampPlaybackScale = (scale: number | null | undefined): number =>
  Math.min(PLAYBACK_SCALE_MAX, Math.max(PLAYBACK_SCALE_MIN, scale ?? DEFAULT_PLAYBACK_SCALE));
