import { DEFAULT_PLAYBACK_SCALE } from '@/features/playback/lib/display-scale';
import {
  DEFAULT_MIC_LATENCY_COMPENSATION_SEC,
  MAX_MIC_LATENCY_COMPENSATION_SEC,
} from '@/features/playback/lib/pitch/constants';
import type { AppConfig } from '@/types/AppConfig';

export { PLAYBACK_SCALE_MAX, PLAYBACK_SCALE_MIN } from '@/features/playback/lib/display-scale';

export type SettingsTab = 'general' | 'playback' | 'analysis';
export type SettingsOption = { value: string; label: string; description?: string };

export const SETTINGS_TABS: { value: SettingsTab; label: string }[] = [
  { value: 'general', label: 'General' },
  { value: 'playback', label: 'Playback' },
  { value: 'analysis', label: 'Analysis' },
];

export const SEPARATORS: SettingsOption[] = [
  {
    value: 'karaoke',
    label: 'UVR Karaoke',
    description: 'Usually separates more cleanly, but can occasionally slip on tricky parts.',
  },
  {
    value: 'demucs',
    label: 'Demucs',
    description:
      'Smoother and more consistent with fewer abrupt artifacts, though slightly less crisp overall.',
  },
];

export const ASR_ENGINES: SettingsOption[] = [
  {
    value: 'whisper',
    label: 'Whisper',
    description: 'Works in any language and lets you pick a model size below.',
  },
  {
    value: 'parakeet',
    label: 'Parakeet v3 (Experimental)',
    description:
      'Much faster and produces its own word timings (skipping alignment), but only covers 25 European languages. Whisper takes over for anything else.',
  },
];

export const ALIGN_BACKENDS: SettingsOption[] = [
  {
    value: 'whisperx',
    label: 'WhisperX',
    description: 'The reliable default, timing words with a proven decoder.',
  },
  {
    value: 'ctc',
    label: 'CTC Forced Alignment (Experimental)',
    description:
      'Calculates word start/end points with a different algorithm, and runs much faster on GPU and Apple Silicon. Falls back to WhisperX if a line trips it up.',
  },
  {
    value: 'qwen',
    label: 'Qwen Forced Alignment (Experimental)',
    description:
      'A fast AI model covering 11 languages. Timing quality varies song to song, but it can do better on Chinese, Japanese, and Korean. Falls back to WhisperX otherwise.',
  },
];

export const MODELS = ['large-v3', 'large-v3-turbo', 'medium', 'small', 'base', 'tiny'];

export const PLAYBACK_MODES: SettingsOption[] = [
  {
    value: 'classic',
    label: 'Classic mode',
    description: 'Playback replaces the menu in the current window.',
  },
  {
    value: 'session',
    label: 'Session mode',
    description: 'Playback uses a separate window while the menu manages the queue.',
  },
];

export const LYRICS_VERTICAL_POSITIONS: SettingsOption[] = [
  { value: 'bottom', label: 'Bottom' },
  { value: 'center', label: 'Center' },
  { value: 'top', label: 'Top' },
];

export const LYRICS_HORIZONTAL_POSITIONS: SettingsOption[] = [
  { value: 'left', label: 'Left' },
  { value: 'center', label: 'Center' },
  { value: 'right', label: 'Right' },
];

/** Pixabay-backed karaoke-video background pools -- download + build-reels
 * run independently per flavor (see `SettingsPage`'s general tab). Shells
 * out to a vendored ffmpeg against the server's own data dir, so this
 * section only renders in the web/server build (`showBackgroundVideos`). */
export const BACKGROUND_VIDEO_FLAVORS: { value: string; label: string }[] = [
  { value: 'nature', label: 'Nature' },
  { value: 'underwater', label: 'Underwater' },
  { value: 'space', label: 'Space' },
];

export const DEFAULTS = {
  separator: 'karaoke',
  asr_engine: 'whisper',
  align_backend: 'whisperx',
  alt_align_backend: 'whisperx',
  vocal_detection_threshold_pct: 0.15,
  whisper_model: 'large-v3',
  beam_size: 8,
  batch_size: 8,
  mic_monitor_gain: 0.65,
  mic_latency_compensation_sec: DEFAULT_MIC_LATENCY_COMPENSATION_SEC,
  use_external_lyrics: false,
  auto_analyze: false,
  restore_analyze: false,
  playback_mode: 'classic',
  lyrics_vertical_position: 'bottom',
  lyrics_horizontal_position: 'center',
  lyrics_scale: DEFAULT_PLAYBACK_SCALE,
  pitch_graph_scale: DEFAULT_PLAYBACK_SCALE,
  parallel_analysis_url: '',
} satisfies Pick<
  AppConfig,
  | 'separator'
  | 'asr_engine'
  | 'align_backend'
  | 'alt_align_backend'
  | 'vocal_detection_threshold_pct'
  | 'whisper_model'
  | 'beam_size'
  | 'batch_size'
  | 'mic_monitor_gain'
  | 'mic_latency_compensation_sec'
  | 'use_external_lyrics'
  | 'auto_analyze'
  | 'restore_analyze'
  | 'playback_mode'
  | 'lyrics_vertical_position'
  | 'lyrics_horizontal_position'
  | 'lyrics_scale'
  | 'pitch_graph_scale'
  | 'parallel_analysis_url'
>;

export const MIC_MONITOR_GAIN_STEP = 0.01;
export const MIC_MONITOR_GAIN_MAX = 2;
export const MIC_LATENCY_STEP = 0.005;
export const MIC_LATENCY_MAX = MAX_MIC_LATENCY_COMPENSATION_SEC;
export const PLAYBACK_SCALE_STEP = 0.05;
// Vocal-detection threshold is stored as a fraction of peak RMS (0-1) but shown
// as a percentage. Capped at 60% since anything higher trims almost everything.
export const VOCAL_THRESHOLD_STEP = 0.01;
export const VOCAL_THRESHOLD_MIN = 0;
export const VOCAL_THRESHOLD_MAX = 0.6;
export const NUMBER_PICKER_SIZE = 16;

export const NAV = {
  tabSegment: 0,
  general: {
    window: 1,
    microphone: 2,
    micMonitorGain: 3,
    micLatency: 4,
    micTest: 5,
    // Server-only (see `showBackgroundVideos`) -- one segment per flavor
    // holding a download + build-reels button pair, indexed via
    // `flavorIndex * 2` / `flavorIndex * 2 + 1`. Appended after every other
    // general field so hiding it (Tauri desktop build) just drops the last
    // stop before the footer instead of shifting any existing NAV indices.
    backgroundVideos: 6,
  },
  playback: {
    mode: 1,
    lyricsVerticalPosition: 2,
    lyricsHorizontalPosition: 3,
    lyricsScale: 4,
    pitchGraphScale: 5,
  },
} as const;

// The Whisper-only "Model size" + "Beam Size" fields sit right after the
// transcription model, so every later field shifts by two segments when
// Parakeet hides them. Fields that aren't rendered map to -1 so focus rings
// never match them. `useExternalLyrics`/`restoreAnalyze`/`altAlignBackend`
// are appended after `batchSize` rather than inserted mid-sequence, so none
// of the fields above have to shift regardless of the Parakeet branch.
// Parallel analysis (`showParallelAnalysis`, server-only -- there's no peer
// for the Tauri desktop app to point at itself) is appended the same way,
// last.
export function getAnalysisNav(isParakeet: boolean, showParallelAnalysis: boolean) {
  const base = isParakeet
    ? {
        separator: 1,
        asrEngine: 2,
        whisperModel: -1,
        beamSize: -1,
        alignBackend: 3,
        autoAnalyze: 4,
        vocalThreshold: 5,
        batchSize: 6,
      }
    : {
        separator: 1,
        asrEngine: 2,
        whisperModel: 3,
        beamSize: 4,
        alignBackend: 5,
        autoAnalyze: 6,
        vocalThreshold: 7,
        batchSize: 8,
      };

  const useExternalLyrics = Math.max(...Object.values(base)) + 1;
  const restoreAnalyze = useExternalLyrics + 1;
  const altAlignBackend = restoreAnalyze + 1;
  const next = altAlignBackend + 1;

  return {
    ...base,
    useExternalLyrics,
    restoreAnalyze,
    altAlignBackend,
    parallelAnalysisEnabled: showParallelAnalysis ? next : -1,
    parallelAnalysisUrl: showParallelAnalysis ? next + 1 : -1,
    parallelAnalysisPing: showParallelAnalysis ? next + 2 : -1,
    parallelAnalysisOnly: showParallelAnalysis ? next + 3 : -1,
  };
}

export function getSettingsStops(
  tab: SettingsTab,
  isParakeet: boolean,
  showParallelAnalysis: boolean,
  showBackgroundVideos: boolean,
) {
  if (tab === 'general') {
    const base = [3, 2, 1, 1, 2, 2];

    return showBackgroundVideos ? [...base, BACKGROUND_VIDEO_FLAVORS.length * 2, 2] : [...base, 2];
  }
  if (tab === 'playback') {
    return [3, 1, 1, 1, 1, 1, 2];
  }

  // The two "2"s before the trailing footer "2" are useExternalLyrics's and
  // restoreAnalyze's own toggles, and the "1" after them is altAlignBackend
  // -- each appended after every existing field rather than inserted
  // mid-sequence, so none of the indices above have to shift. Parallel
  // analysis's own [2 (Off/On), 1 (URL field), 1 (Ping button), 2 (only
  // Off/On)] is appended the same way, only when `showParallelAnalysis` is
  // true.
  const base = isParakeet
    ? [3, 1, 1, 1, 2, 1, NUMBER_PICKER_SIZE, 2, 2, 1]
    : [3, 1, 1, 1, NUMBER_PICKER_SIZE, 1, 2, 1, NUMBER_PICKER_SIZE, 2, 2, 1];

  return showParallelAnalysis ? [...base, 2, 1, 1, 2, 2] : [...base, 2];
}
