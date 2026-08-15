import { loadSongsByHashes } from "@/bridge/songs";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useDialog, type DialogMode } from "@/hooks/use-dialog";
import { useDialogNav } from "@/hooks/navigation/use-dialog-nav";
import { cn } from "@/lib/utils";
import { useProfiles } from "@/queries/use-profiles";
import type { ScoreRecord } from "@/types/ScoreRecord";
import type { Song } from "@/types/Song";
import { topScoresForSong } from "@/utils/playback/result";
import { useQuery } from "@tanstack/react-query";
import { TrophyIcon } from "lucide-react";
import { useMemo, useRef } from "react";

const TOP_LIMIT = 10;
const RING = "ring-2 ring-primary";
const NO_FOCUS_RING = "focus-visible:ring-0 focus-visible:border-transparent";
const EMPTY_SCORES: ScoreRecord[] = [];

function topScoreRecords(records: ScoreRecord[]): ScoreRecord[] {
  return records
    .map((record, index) => ({ record, index }))
    .sort((a, b) => b.record.score - a.record.score || a.index - b.index)
    .slice(0, TOP_LIMIT)
    .map(({ record }) => record);
}

type SongLeaderboardMode = Extract<DialogMode, { mode: "song-leaderboard" }>;
type LeaderboardMode = "leaderboards" | SongLeaderboardMode;

function isLeaderboardMode(mode: DialogMode): mode is LeaderboardMode {
  return (
    mode === "leaderboards" ||
    (typeof mode === "object" && mode !== null && mode.mode === "song-leaderboard")
  );
}

function useRetainedLeaderboardMode(mode: DialogMode): LeaderboardMode {
  const retainedMode = useRef<LeaderboardMode>("leaderboards");

  if (isLeaderboardMode(mode)) {
    retainedMode.current = mode;
  }

  return retainedMode.current;
}

interface LeaderboardSongNameProps {
  song?: Song;
  loading: boolean;
  songHash: string;
}

function LeaderboardSongName({ song, loading, songHash }: LeaderboardSongNameProps) {
  if (song) {
    return (
      <div className="min-w-0">
        <div className="truncate font-medium">{song.title || songHash}</div>
        <div className="truncate text-muted-foreground">{song.artist || "Unknown artist"}</div>
      </div>
    );
  }

  if (loading) {
    return "Loading…";
  }

  return `Unknown song (${songHash.slice(0, 8)})`;
}

export const LeaderboardsDialog = () => {
  const { mode, close } = useDialog();
  const { data: profiles } = useProfiles();
  const containerRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const displayedMode = useRetainedLeaderboardMode(mode);
  const open = isLeaderboardMode(mode);
  const globalOpen = displayedMode === "leaderboards";
  const songMode = typeof displayedMode === "object" ? displayedMode : null;
  const scores = profiles?.scores ?? EMPTY_SCORES;

  const globalBoard = useMemo(() => topScoreRecords(scores), [scores]);
  const songBoard = useMemo(
    () => (songMode ? topScoresForSong(scores, songMode.song.file_hash, TOP_LIMIT) : []),
    [scores, songMode],
  );
  const targetSongHashes = useMemo(
    () => [...new Set(globalBoard.map((record) => record.song_hash))].sort(),
    [globalBoard],
  );
  const { data: leaderboardSongs, isLoading: songsLoading } = useQuery({
    queryKey: ["leaderboard-songs", targetSongHashes],
    queryFn: async () => {
      const songs = await loadSongsByHashes(targetSongHashes);
      return new Map(songs.map((song) => [song.file_hash, song]));
    },
    enabled: globalOpen && targetSongHashes.length > 0,
  });

  const { focusedIndex, focusSegment } = useDialogNav({
    open,
    itemCount: 1,
    containerRef,
    onBack: close,
    onAction: (_segment, _slot, action) => {
      if (!action.up && !action.down) return false;
      scrollRef.current?.scrollBy({ top: action.down ? 48 : -48, behavior: "smooth" });
      return true;
    },
  });

  const songTitle = songMode?.song.title;
  const songArtist = songMode?.song.artist || "Unknown artist";
  const boardEmpty = globalOpen ? globalBoard.length === 0 : songBoard.length === 0;

  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && close()}>
      <DialogContent
        showCloseButton={false}
        className={cn(
          "flex max-h-[calc(100svh-2rem)] min-h-0 flex-col gap-0 overflow-hidden p-0",
          globalOpen ? "sm:max-w-2xl" : "sm:max-w-lg",
        )}
      >
        <div ref={containerRef} className="contents">
          <DialogHeader className="shrink-0 gap-1 border-b p-4 pr-10">
            <DialogTitle className="flex min-w-0 items-start gap-2 text-base">
              <TrophyIcon className="size-4 shrink-0 self-center text-primary" />
              <span className="min-w-0">
                <span className="block truncate">{globalOpen ? "Leaderboards" : songTitle}</span>
                {!globalOpen && (
                  <span className="block truncate text-xs font-normal text-muted-foreground">
                    {songArtist}
                  </span>
                )}
              </span>
            </DialogTitle>
            <DialogDescription>
              {globalOpen
                ? "Top score records across every profile and song."
                : "Best score for each profile."}
            </DialogDescription>
          </DialogHeader>

          <div
            ref={scrollRef}
            className="min-h-0 flex-1 overscroll-contain overflow-auto px-2 sm:px-4"
          >
            {boardEmpty && (
              <p className="py-10 text-center text-sm text-muted-foreground">No scores yet.</p>
            )}
            {!boardEmpty && globalOpen && (
              <Table className="table-fixed">
                <TableHeader className="sticky top-0 z-10 bg-background">
                  <TableRow className="hover:bg-transparent">
                    <TableHead className="h-9 w-8 px-1 text-xs">#</TableHead>
                    <TableHead className="h-9 text-xs">Song</TableHead>
                    <TableHead className="h-9 w-[28%] text-xs">Profile</TableHead>
                    <TableHead className="h-9 w-16 text-right text-xs">Score</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {globalBoard.map((record, index) => (
                    <TableRow
                      key={`${record.song_hash}-${record.profile}-${index}`}
                      className="hover:bg-transparent"
                    >
                      <TableCell className="px-1 py-2 text-xs tabular-nums">{index + 1}</TableCell>
                      <TableCell className="max-w-0 py-2 text-xs">
                        <LeaderboardSongName
                          song={leaderboardSongs?.get(record.song_hash)}
                          loading={songsLoading}
                          songHash={record.song_hash}
                        />
                      </TableCell>
                      <TableCell className="max-w-0 truncate py-2 text-xs" title={record.profile}>
                        {record.profile}
                      </TableCell>
                      <TableCell className="py-2 text-right text-xs font-medium tabular-nums">
                        {record.score}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
            {!boardEmpty && !globalOpen && (
              <Table className="table-fixed">
                <TableHeader className="sticky top-0 z-10 bg-background">
                  <TableRow className="hover:bg-transparent">
                    <TableHead className="h-9 w-8 px-1 text-xs">#</TableHead>
                    <TableHead className="h-9 text-xs">Profile</TableHead>
                    <TableHead className="h-9 w-28 text-right text-xs">Score</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {songBoard.map(({ profile, score }, index) => (
                    <TableRow key={profile} className="hover:bg-transparent">
                      <TableCell className="px-1 py-2 text-xs tabular-nums">{index + 1}</TableCell>
                      <TableCell className="max-w-0 truncate py-2 text-xs" title={profile}>
                        {profile}
                      </TableCell>
                      <TableCell className="py-2 text-right text-xs font-medium tabular-nums">
                        {score}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </div>

          <DialogFooter className="shrink-0 border-t p-3">
            <Button
              variant="outline"
              onClick={close}
              onMouseEnter={() => focusSegment(0)}
              className={cn(NO_FOCUS_RING, open && focusedIndex === 0 && RING)}
            >
              Close
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
};
