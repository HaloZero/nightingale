import type { Song } from "@/types/Song";
import type { SongItemProps } from "../types";
import { SongGridCard } from "./song-grid-card";
import { useBestScoresBySongForActiveProfile } from "@/hooks/use-best-scores-by-song";

interface SongGridProps {
  songs: Song[];
  getItemProps: (song: Song, index: number) => SongItemProps;
}

export const SongGrid = ({ songs, getItemProps }: SongGridProps) => {
  const bestScores = useBestScoresBySongForActiveProfile();

  return (
    <div
      role="list"
      className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,18rem),1fr))] gap-3 p-1"
    >
      {songs.map((song, index) => (
        <SongGridCard
          key={song.file_hash}
          {...getItemProps(song, index)}
          bestScore={bestScores.get(song.file_hash)}
        />
      ))}
    </div>
  );
};
