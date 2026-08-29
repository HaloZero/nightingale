import { ANALYSIS_STATUS_STYLES } from "@/lib/analysis-status-styles";
import type { QueuedStatus } from "@/types/QueuedStatus";
import type { Song } from "@/types/Song";

export type SongStatusInfo = {
  label: string;
  variant: "default" | "secondary" | "destructive" | "outline";
  className?: string;
  isAnalyzing?: boolean;
  isReady?: boolean;
  isQueued?: boolean;
};

export function formatTranscriptSource(source: Song["transcript_source"]): string {
  if (source === "Lyrics") return "AI aligned";
  if (source === "Usdx") return "USDX";
  if (source === "Lrc") return "LRC";
  return "AI generated";
}

/** Label for `Song.align_backend` -- `null` for `Lrc`/`Usdx` sources and for
 * songs analyzed before this field existed (see `Song.align_backend`'s doc
 * comment), so callers should hide the row entirely rather than show this
 * fallback. */
export function formatAlignBackend(backend: Song["align_backend"]): string {
  if (backend === "ctc") return "CTC Forced Alignment";
  if (backend === "qwen") return "Qwen Forced Alignment";
  if (backend === "whisperx") return "WhisperX";
  return "Unknown";
}

export function getSongStatusInfo(isAnalyzed: boolean, queueStatus?: QueuedStatus): SongStatusInfo {
  if (queueStatus === "Queued") {
    return {
      label: "Queued",
      variant: "secondary",
      className: ANALYSIS_STATUS_STYLES.queued,
      isQueued: true,
    };
  }

  if (typeof queueStatus === "object") {
    if ("Analyzing" in queueStatus) {
      return {
        label: `Analyzing ${queueStatus.Analyzing}%`,
        variant: "default",
        className: `${ANALYSIS_STATUS_STYLES.analysing} animate-pulse`,
        isAnalyzing: true,
      };
    }
    if ("Failed" in queueStatus) return { label: "Failed", variant: "destructive" };
  }

  if (isAnalyzed) {
    return {
      label: "Ready",
      variant: "default",
      className: ANALYSIS_STATUS_STYLES.analysed,
      isReady: true,
    };
  }

  return { label: "Not analyzed", variant: "outline" };
}
