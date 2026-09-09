import { useBestScoresBySongForActiveProfile } from '@/features/profiles/hooks/use-best-scores-by-song';
import type { Song } from '@/types/Song';

import { songKey } from '../shared/song-key';
import type { SongItemProps } from '../types';
import { SongGridCard } from './song-grid-card';

type SongGridProps = {
  songs: Song[];
  getItemProps: (song: Song, index: number) => SongItemProps;
};

export const SongGrid = ({ songs, getItemProps }: SongGridProps) => {
  const bestScores = useBestScoresBySongForActiveProfile();

  return (
    <ul className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,18rem),1fr))] gap-3 p-1">
      {songs.map((song, index) => (
        <li key={songKey(song)}>
          <SongGridCard {...getItemProps(song, index)} bestScore={bestScores.get(song.file_hash)} />
        </li>
      ))}
    </ul>
  );
};
