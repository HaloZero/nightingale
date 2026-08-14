import { ANALYSIS_STATUS_STYLES } from "@/lib/analysis-status-styles";
import type { QueuedStatus } from "@/types/QueuedStatus";
import type { Song } from "@/types/Song";

export type SongStatusInfo = {
  label: string;
  variant: "default" | "secondary" | "destructive" | "outline";
  className?: string;
  isAnalyzing?: boolean;
  isReady?: boolean;
};

export function formatTranscriptSource(source: Song["transcript_source"]): string {
  if (source === "Lyrics") return "AI aligned";
  if (source === "Usdx") return "USDX";
  if (source === "Lrc") return "LRC";
  return "AI transcribed";
}

export function getSongStatusInfo(isAnalyzed: boolean, queueStatus?: QueuedStatus): SongStatusInfo {
  if (queueStatus === "Queued") {
    return { label: "Queued", variant: "secondary", className: ANALYSIS_STATUS_STYLES.queued };
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
