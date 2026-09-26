import { ListPlusIcon } from 'lucide-react';

import { useAddPlaybackQueueEntry } from '@/features/playback-queue/use-playback-queue';
import { usePlaybackLauncher } from '@/features/playback/hooks/use-playback-launcher';
import { Button } from '@/shared/components/ui/button';
import { formatSeconds } from '@/shared/utils/format-duration';
import type { Song } from '@/types/Song';

type SearchResultRowProps = {
  song: Song;
};

export const SearchResultRow = ({ song }: SearchResultRowProps) => {
  const { launch, reserveTarget } = usePlaybackLauncher();
  const { mutate: addToQueue, isLoading: isQueuing } = useAddPlaybackQueueEntry();

  const handlePlay = () => {
    const target = reserveTarget();
    if (target === undefined) {
      return;
    }
    void launch({ song, queuePlayback: false }, target);
  };

  return (
    <li className="flex items-center gap-3 rounded-md hover:bg-accent/50">
      <button
        type="button"
        disabled={!song.is_analyzed}
        onClick={handlePlay}
        aria-label={`Play ${song.title}`}
        className="min-w-0 flex-1 rounded-md px-3 py-2 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default"
      >
        <p className="truncate font-medium">{song.title}</p>
        <p className="truncate text-sm text-muted-foreground">
          {song.artist} · {formatSeconds(song.duration_secs)}
        </p>
      </button>
      {song.is_analyzed ? (
        <Button
          variant="outline"
          size="icon-sm"
          disabled={isQueuing}
          onClick={() => addToQueue({ song, tempo: song.tempo, keyOffset: song.key_offset })}
          aria-label={`Add ${song.title} to playback queue`}
          title="Add to queue"
          className="mr-3 shrink-0"
        >
          <ListPlusIcon />
        </Button>
      ) : (
        <span className="mr-3 shrink-0 text-xs text-muted-foreground">Not analyzed</span>
      )}
    </li>
  );
};
