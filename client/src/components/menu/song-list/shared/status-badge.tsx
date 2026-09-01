import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type { QueuedStatus } from "@/types/QueuedStatus";
import type { Song } from "@/types/Song";
import { LoaderCircleIcon, MicIcon, YoutubeIcon } from "lucide-react";
import { formatTranscriptSource, getSongStatusInfo } from "./song-status";

/** Reused by both the song-list "Analysis status" column and the song
 * details sidebar header (`SongDetailsHeader`) -- one component, so the
 * karaoke-video icons below show up in both places automatically. */
export function StatusBadge({ song, queueStatus }: { song: Song; queueStatus?: QueuedStatus }) {
  const status = getSongStatusInfo(song.is_analyzed, queueStatus);
  const source = status.isReady ? ` (${formatTranscriptSource(song.transcript_source)})` : "";

  return (
    <span className="inline-flex items-center gap-1.5">
      <Badge variant={status.variant} className={cn("border-foreground/15", status.className)}>
        {status.isAnalyzing ? <LoaderCircleIcon className="animate-spin" /> : null}
        {status.label}
        {source}
      </Badge>
      {song.karaoke_video_version > 0 || song.youtube_karaoke_video_version > 0 ? (
        <span className="inline-flex items-center gap-1 text-muted-foreground">
          {song.karaoke_video_version > 0 ? (
            <MicIcon
              className="size-3.5"
              aria-label={`Karaoke video available (v${song.karaoke_video_version})`}
            >
              <title>Karaoke video available (v{song.karaoke_video_version})</title>
            </MicIcon>
          ) : null}
          {song.youtube_karaoke_video_version > 0 ? (
            <YoutubeIcon
              className="size-3.5"
              aria-label={`YouTube karaoke video available (v${song.youtube_karaoke_video_version})`}
            >
              <title>YouTube karaoke video available (v{song.youtube_karaoke_video_version})</title>
            </YoutubeIcon>
          ) : null}
        </span>
      ) : null}
    </span>
  );
}
