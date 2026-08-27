import type { Song } from "@/types/Song";
import {
  AlignLeftIcon,
  AudioLinesIcon,
  ImageIcon,
  LanguagesIcon,
  ListXIcon,
  MicIcon,
  PencilLineIcon,
  RefreshCwIcon,
  Repeat2Icon,
  RotateCcwIcon,
  Trash2Icon,
  VideoIcon,
  YoutubeIcon,
} from "lucide-react";
import type { SongStatusInfo } from "../shared/song-status";
import type { ActionItemProps } from "./action-item";

type AnalysisHandler = (fileHash: string) => void | Promise<void>;

interface AnalysisHandlers {
  enqueueOne: AnalysisHandler;
  removeFromQueue: AnalysisHandler;
  deleteSongCache: AnalysisHandler;
  reanalyzeFull: AnalysisHandler;
  reanalyzeTranscript: AnalysisHandler;
  realign: AnalysisHandler;
  reanalyzeForceTranscribe: AnalysisHandler;
  refreshMetadata: AnalysisHandler;
}

interface BuildActionGroupsParams {
  song: Song;
  status: SongStatusInfo;
  analysisBusy: boolean;
  supportsAnalysisActions: boolean;
  analysis: AnalysisHandlers;
  onEditLyrics: () => void;
  onChangeLanguage: () => void;
  run: (message: string, action: () => void | Promise<void>) => () => Promise<void>;
  /** Server-only feature (casting/rendering both live server-side); hidden in the Tauri desktop build. */
  showKaraokeVideoActions: boolean;
  onRenderKaraokeVideo: () => void;
  onForceRerenderKaraokeVideo: () => void;
  onFetchYoutubeKaraokeVideo: () => void;
  onForceFetchYoutubeKaraokeVideo: () => void;
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
  showKaraokeVideoActions,
  onRenderKaraokeVideo,
  onForceRerenderKaraokeVideo,
  onFetchYoutubeKaraokeVideo,
  onForceFetchYoutubeKaraokeVideo,
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

    // Mirrors the bulk "Remove from queue" action's eligibility (still
    // pending, not actively being analyzed) -- see `remove_from_queue_all`
    // in app-core's analyzer.rs.
    if (status.isQueued) {
      notReadyGroup.push({
        icon: ListXIcon,
        title: "Remove from queue",
        description: "Cancel analysis and take this song out of the queue.",
        onClick: run(`Removed "${song.title}" from the queue`, () =>
          analysis.removeFromQueue(song.file_hash),
        ),
      });
    }

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

    // Karaoke video generation only needs a transcript (i.e. `status.isReady`,
    // already required to reach this branch), independent of transcript_source
    // -- applies equally to LRC-provided and AI-analyzed songs. Server-only, so
    // hidden entirely in the Tauri desktop build (see `showKaraokeVideoActions`).
    if (showKaraokeVideoActions) {
      groups.push([
        {
          icon: VideoIcon,
          title: "Render karaoke video",
          description:
            "Render a background+lyrics video for casting. Skipped if already up to date.",
          onClick: onRenderKaraokeVideo,
        },
        {
          icon: Repeat2Icon,
          title: "Force re-render karaoke video",
          description: "Regenerate unconditionally, with a freshly picked background.",
          onClick: onForceRerenderKaraokeVideo,
        },
        {
          icon: YoutubeIcon,
          title: "Fetch YouTube karaoke video",
          description:
            "Look up the official music video and re-render using it as the background, if it can be synced to the song.",
          onClick: onFetchYoutubeKaraokeVideo,
        },
        {
          icon: RotateCcwIcon,
          title: "Force re-fetch YouTube karaoke video",
          description: "Re-download the source video and re-render unconditionally.",
          onClick: onForceFetchYoutubeKaraokeVideo,
        },
      ]);
    }

    // Independent of the analysis pipeline (title/artist/album/duration/
    // cover/lyrics-source-flags, re-read straight from the file) -- not
    // gated by transcript_source since it applies equally to LRC-provided
    // songs. USDX songs get their metadata from the chart file, not audio
    // tags, so there's nothing here to re-read for them.
    if (!song.usdx) {
      groups.push([
        {
          icon: ImageIcon,
          title: "Refresh metadata",
          description: "Re-read title, artist, album, cover art, and lyrics source from the file.",
          onClick: run(`Refreshed metadata for "${song.title}"`, () =>
            analysis.refreshMetadata(song.file_hash),
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
