import type { Song } from "@/types/Song";
import {
  AlignLeftIcon,
  AudioLinesIcon,
  ImageIcon,
  LanguagesIcon,
  MicIcon,
  PencilLineIcon,
  RefreshCwIcon,
  Trash2Icon,
} from "lucide-react";
import type { SongStatusInfo } from "../shared/song-status";
import type { ActionItemProps } from "./action-item";

type AnalysisHandler = (fileHash: string) => void | Promise<void>;

interface AnalysisHandlers {
  enqueueOne: AnalysisHandler;
  deleteSongCache: AnalysisHandler;
  reanalyzeFull: AnalysisHandler;
  reanalyzeTranscript: AnalysisHandler;
  realign: AnalysisHandler;
  reanalyzeForceTranscribe: AnalysisHandler;
  refreshMetadata: (fileHash: string) => Promise<boolean | undefined>;
}

interface BuildActionGroupsParams {
  song: Song;
  status: SongStatusInfo;
  analysisBusy: boolean;
  supportsAnalysisActions: boolean;
  analysis: AnalysisHandlers;
  onEditLyrics: () => void;
  onChangeLanguage: () => void;
  run: (
    message: string,
    action: () => void | boolean | undefined | Promise<void | boolean | undefined>,
    onFalse?: string,
  ) => () => Promise<void>;
}

export function buildActionGroups({
  song,
  status,
  analysisBusy,
  supportsAnalysisActions,
  analysis,
  onEditLyrics,
  onChangeLanguage,
  run,
}: BuildActionGroupsParams): ActionItemProps[][] {
  const groups: ActionItemProps[][] = [];

  const supportsProvideLyrics = song.transcript_source !== "Usdx";

  if (!status.isReady) {
    const notReadyGroup: ActionItemProps[] = [
      {
        icon: AudioLinesIcon,
        title: analysisBusy ? "Analysis in progress" : "Analyze song",
        description: "Prepare lyrics, timing, key, tempo, and stems.",
        disabled: analysisBusy,
        onClick: () => analysis.enqueueOne(song.file_hash),
      },
    ];

    if (supportsProvideLyrics) {
      notReadyGroup.push({
        icon: PencilLineIcon,
        title: "Provide lyrics",
        description: "Paste timed LRC, or lyrics to align.",
        disabled: analysisBusy,
        onClick: onEditLyrics,
      });
    }

    groups.push(notReadyGroup);
  }

  if (supportsAnalysisActions) {
    // LRC-provided songs have no AI-generated stems/timing to rebuild, so the
    // realign/refetch/transcribe actions don't apply. Offer editing the LRC and
    // an explicit opt-in to replace it with full AI analysis instead.
    if (song.transcript_source === "Lrc") {
      groups.push([
        {
          icon: PencilLineIcon,
          title: "Edit lyrics (LRC)",
          description: "Replace or re-time the provided LRC.",
          onClick: onEditLyrics,
        },
        {
          icon: AudioLinesIcon,
          title: "Analyze with AI",
          description: "Replace the LRC with AI stems, lyrics, timing, and key.",
          onClick: run(`Analyzing "${song.title}" with AI`, () =>
            analysis.reanalyzeFull(song.file_hash),
          ),
        },
      ]);
    } else {
      groups.push([
        {
          icon: AlignLeftIcon,
          title: "Realign",
          description: "Rebuild timing from the current lyrics.",
          onClick: run(`Realigning "${song.title}"`, () => analysis.realign(song.file_hash)),
        },
        {
          icon: RefreshCwIcon,
          title: "Refetch lyrics & align",
          description: "Fetch fresh lyrics, then rebuild timing.",
          onClick: run(`Refetching lyrics & aligning "${song.title}"`, () =>
            analysis.reanalyzeTranscript(song.file_hash),
          ),
        },
        {
          icon: MicIcon,
          title: "Force transcribe",
          description: "Ignore online lyrics and transcribe the vocals.",
          onClick: run(`Force transcribing "${song.title}"`, () =>
            analysis.reanalyzeForceTranscribe(song.file_hash),
          ),
        },
        {
          icon: AudioLinesIcon,
          title: "Full reanalysis",
          description: "Recreate stems, lyrics, timing, key, and tempo.",
          onClick: run(`Full reanalysis (w/ stems) for "${song.title}"`, () =>
            analysis.reanalyzeFull(song.file_hash),
          ),
        },
      ]);

      groups.push([
        {
          icon: PencilLineIcon,
          title: "Edit lyrics",
          description: "Correct the words and rebuild their timing.",
          onClick: onEditLyrics,
        },
        {
          icon: LanguagesIcon,
          title: "Change language",
          description: "Set the language and choose how to reprocess.",
          onClick: onChangeLanguage,
        },
      ]);
    }

    if (!song.usdx) {
      groups.push([
        {
          icon: ImageIcon,
          title: "Refresh metadata",
          description:
            "Reload title, artist, album, duration, and cover art from the library source.",
          onClick: run(
            `Refreshed metadata for "${song.title}"`,
            () => analysis.refreshMetadata(song.file_hash),
            `Nothing to refresh for "${song.title}"`,
          ),
        },
      ]);
    }

    groups.push([
      {
        icon: Trash2Icon,
        title: "Delete cache",
        description: "Remove every generated file for this song.",
        destructive: true,
        onClick: run(`Cache deleted for "${song.title}"`, () =>
          analysis.deleteSongCache(song.file_hash),
        ),
      },
    ]);
  }

  return groups;
}
