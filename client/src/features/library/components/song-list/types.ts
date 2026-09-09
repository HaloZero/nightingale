import type { QueuedStatus } from '@/types/QueuedStatus';
import type { Song } from '@/types/Song';

export type SongItemProps = {
  song: Song;
  queueStatus?: QueuedStatus;
  index: number;
  isFocused: boolean;
  isSelected: boolean;
  onSelect: () => void;
};
