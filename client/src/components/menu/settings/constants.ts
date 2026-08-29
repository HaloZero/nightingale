import {
  DEFAULT_MIC_LATENCY_COMPENSATION_SEC,
  MAX_MIC_LATENCY_COMPENSATION_SEC,
} from "@/lib/pitch/constants";
import type { AppConfig } from "@/types/AppConfig";

export type SettingsTab = "general" | "analysis";
export type SettingsOption = { value: string; label: string; description?: string };

export const SETTINGS_TABS: { value: SettingsTab; label: string }[] = [
  { value: "general", label: "General" },
  { value: "analysis", label: "Analysis" },
];

export const BACKGROUND_VIDEO_FLAVORS: { value: string; label: string }[] = [
  { value: "nature", label: "Nature" },
  { value: "underwater", label: "Underwater" },
  { value: "space", label: "Space" },
];

export const SEPARATORS: SettingsOption[] = [
  {
    value: "karaoke",
    label: "UVR Karaoke",
    description: "Usually separates more cleanly, but can occasionally slip on tricky parts.",
  },
  {
    value: "demucs",
    label: "Demucs",
    description:
      "Smoother and more consistent with fewer abrupt artifacts, though slightly less crisp overall.",
  },
];

export const ASR_ENGINES: SettingsOption[] = [
  {
    value: "whisper",
    label: "Whisper",
    description: "Works in any language and lets you pick a model size below.",
  },
  {
    value: "parakeet",
    label: "Parakeet v3 (Experimental)",
    description:
      "Much faster and produces its own word timings (skipping alignment), but only covers 25 European languages. Whisper takes over for anything else.",
  },
  {
    value: "whisper_mlx",
    label: "Whisper MLX (Experimental)",
    description:
      "Runs Whisper natively on Apple Silicon's GPU via MLX instead of the CPU-only fallback WhisperX is stuck with on Mac. Same model sizes below. Apple Silicon only — falls back to Whisper elsewhere.",
  },
];

export const ALIGN_BACKENDS: SettingsOption[] = [
  {
    value: "whisperx",
    label: "WhisperX",
    description: "The reliable default, timing words with a proven decoder.",
  },
  {
    value: "ctc",
    label: "CTC Forced Alignment (Experimental)",
    description:
      "Calculates word start/end points with a different algorithm, and runs much faster on GPU and Apple Silicon. Falls back to WhisperX if a line trips it up.",
  },
  {
    value: "qwen",
    label: "Qwen Forced Alignment (Experimental)",
    description:
      "A fast AI model covering 11 languages. Timing quality varies song to song, but it can do better on Chinese, Japanese, and Korean. Falls back to WhisperX otherwise.",
  },
];

export const MODELS = ["large-v3", "large-v3-turbo", "medium", "small", "base", "tiny"];

export const LYRICS_VERTICAL_POSITIONS: SettingsOption[] = [
  { value: "bottom", label: "Bottom" },
  { value: "center", label: "Center" },
  { value: "top", label: "Top" },
];

export const LYRICS_HORIZONTAL_POSITIONS: SettingsOption[] = [
  { value: "left", label: "Left" },
  { value: "center", label: "Center" },
  { value: "right", label: "Right" },
];

export const DEFAULTS = {
  separator: "karaoke",
  asr_engine: "whisper",
  align_backend: "whisperx",
  alt_align_backend: "whisperx",
  vocal_detection_threshold_pct: 0.15,
  whisper_model: "large-v3",
  beam_size: 8,
  batch_size: 8,
  mic_monitor_gain: 0.65,
  mic_latency_compensation_sec: DEFAULT_MIC_LATENCY_COMPENSATION_SEC,
  use_external_lyrics: false,
  auto_analyze: false,
  restore_analyze: false,
  track_analysis_timings: true,
  lyrics_vertical_position: "bottom",
  lyrics_horizontal_position: "center",
  parallel_analysis_enabled: false,
  parallel_analysis_url: "",
  parallel_analysis_only: false,
} satisfies Pick<
  AppConfig,
  | "separator"
  | "asr_engine"
  | "align_backend"
  | "alt_align_backend"
  | "vocal_detection_threshold_pct"
  | "whisper_model"
  | "beam_size"
  | "batch_size"
  | "mic_monitor_gain"
  | "mic_latency_compensation_sec"
  | "use_external_lyrics"
  | "auto_analyze"
  | "restore_analyze"
  | "track_analysis_timings"
  | "lyrics_vertical_position"
  | "lyrics_horizontal_position"
  | "parallel_analysis_enabled"
  | "parallel_analysis_url"
  | "parallel_analysis_only"
>;

export const MIC_MONITOR_GAIN_STEP = 0.01;
export const MIC_MONITOR_GAIN_MAX = 2;
export const MIC_LATENCY_STEP = 0.005;
export const MIC_LATENCY_MAX = MAX_MIC_LATENCY_COMPENSATION_SEC;
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
    lyricsVerticalPosition: 5,
    lyricsHorizontalPosition: 6,
    pixabayRotation: 7,
    backgroundVideos: 8,
  },
} as const;

// The Whisper-only "Model size" + "Beam Size" fields sit right after the
// transcription model, so every later field shifts by two segments when
// Parakeet hides them. Fields that aren't rendered map to -1 so focus rings
// never match them. `showParallelAnalysis` is `false` in the Tauri desktop
// build (the section isn't rendered there -- server-only feature), so its
// three segments are similarly omitted/`-1`'d rather than shifting anything.
export function getAnalysisNav(isParakeet: boolean, showParallelAnalysis: boolean) {
  const base = isParakeet
    ? {
        separator: 1,
        asrEngine: 2,
        whisperModel: -1,
        beamSize: -1,
        alignBackend: 3,
        autoAnalyze: 4,
        trackTimings: 5,
        vocalThreshold: 6,
        batchSize: 7,
        useExternalLyrics: 8,
        restoreAnalyze: 9,
        altAlignBackend: 10,
      }
    : {
        separator: 1,
        asrEngine: 2,
        whisperModel: 3,
        beamSize: 4,
        alignBackend: 5,
        autoAnalyze: 6,
        trackTimings: 7,
        vocalThreshold: 8,
        batchSize: 9,
        useExternalLyrics: 10,
        restoreAnalyze: 11,
        altAlignBackend: 12,
      };

  const next = Math.max(...Object.values(base)) + 1;

  return {
    ...base,
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
  if (tab === "general") {
    // Same append-at-the-end trick as parallel analysis below: the
    // background videos field (server-only, gated by `!isTauri`) sits after
    // every other general field, so hiding it just drops the last stop
    // before the footer instead of shifting any existing NAV indices. Its
    // one segment holds a download + build-reels button pair per flavor in
    // `BACKGROUND_VIDEO_FLAVORS`.
    const base = [2, 2, 1, 1, 2, 1, 1];

    return showBackgroundVideos ? [...base, BACKGROUND_VIDEO_FLAVORS.length * 2, 2] : [...base, 2];
  }

  // The two "2"s before the trailing footer "2" are useExternalLyrics's and
  // restoreAnalyze's own toggles -- each appended after every existing field
  // rather than inserted mid-sequence, so none of the indices above have to
  // shift. footerSegment (Restore Defaults / Close) is derived as
  // `stops.length - 1`, so it stays correct automatically. Parallel
  // analysis's own [2 (Off/On), 1 (URL field), 1 (Ping button), 2 (only
  // Off/On)] is appended the same way, only when `showParallelAnalysis` is
  // true.
  // altAlignBackend (a select, 1 stop) is appended the same way, right after
  // restoreAnalyze and before the parallel-analysis block.
  const base = isParakeet
    ? [2, 1, 1, 1, 2, 2, 1, NUMBER_PICKER_SIZE, 2, 2, 1]
    : [2, 1, 1, 1, NUMBER_PICKER_SIZE, 1, 2, 2, 1, NUMBER_PICKER_SIZE, 2, 2, 1];

  return showParallelAnalysis ? [...base, 2, 1, 1, 2, 2] : [...base, 2];
}
