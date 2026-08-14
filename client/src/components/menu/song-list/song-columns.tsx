import type { QueuedStatus } from "@/types/QueuedStatus";
import type { Song } from "@/types/Song";
import type { ReactNode } from "react";
import { formatSeconds } from "@/utils/format-duration";
import { AlbumArt } from "./shared/album-art";
import { LanguageBadge } from "./shared/language-badge";
import { StatusBadge } from "./shared/status-badge";
import { Stars } from "@/components/shared/stars";

interface SongColumn {
  id: string;
  header: ReactNode;
  thClassName: string;
  tdClassName: string;
  cell: (song: Song, queueStatus?: QueuedStatus, bestScore?: number) => ReactNode;
}

export const SONG_COLUMNS: SongColumn[] = [
  {
    id: "thumbnail",
    header: <span className="sr-only">Cover</span>,
    thClassName: "song-table__thumbnail px-2 py-2 font-medium",
    tdClassName: "song-table__thumbnail py-1.5 pr-2 pl-2",
    cell: (song) => (
      <AlbumArt
        song={song}
        className="size-10 rounded-md"
        fallbackIconClassName="size-5"
        showVideoBadge
      />
    ),
  },
  {
    id: "song",
    header: "Song",
    thClassName: "song-table__song px-2 py-2 font-medium",
    tdClassName: "song-table__song px-2 py-2 align-middle font-medium",
    cell: (song, _queueStatus, bestScore) => (
      <div className="min-w-0">
        <div className="flex h-5 min-w-0 items-center gap-2">
          <span className="min-w-0 truncate leading-5">{song.title}</span>
          <LanguageBadge language={song.language} />
        </div>
        {bestScore === undefined ? null : <Stars score={bestScore} size="sm" className="mt-0.5" />}
      </div>
    ),
  },
  {
    id: "band",
    header: "Artist",
    thClassName: "song-table__band px-2 py-2 font-medium",
    tdClassName: "song-table__band px-2 py-2 text-muted-foreground",
    cell: (song) => <span className="block truncate">{song.artist || "—"}</span>,
  },
  {
    id: "album",
    header: "Album",
    thClassName: "song-table__album px-2 py-2 font-medium",
    tdClassName: "song-table__album px-2 py-2 text-muted-foreground",
    cell: (song) => <span className="block truncate">{song.album || "—"}</span>,
  },
  {
    id: "duration",
    header: "Duration",
    thClassName: "song-table__duration px-2 py-2 font-medium",
    tdClassName:
      "song-table__duration px-2 py-2 font-variant-numeric tabular-nums text-muted-foreground",
    cell: (song) => formatSeconds(song.duration_secs),
  },
  {
    id: "status",
    header: "Analysis status",
    thClassName: "song-table__status px-2 py-2 text-right font-medium",
    tdClassName: "song-table__status px-2 py-2 text-right",
    cell: (song, queueStatus) => <StatusBadge song={song} queueStatus={queueStatus} />,
  },
];
