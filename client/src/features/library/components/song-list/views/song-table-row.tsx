import { memo, type KeyboardEvent } from 'react';

import { cn } from '@/shared/utils/cn';

import { SONG_COLUMNS } from '../song-columns';
import type { SongItemProps } from '../types';

type SongTableRowProps = {
  bestScore?: number;
} & SongItemProps;

export const SongTableRow = memo(
  ({ song, queueStatus, index, isFocused, isSelected, onSelect, bestScore }: SongTableRowProps) => {
    const onKeyDown = (event: KeyboardEvent<HTMLTableRowElement>) => {
      if (event.key !== 'Enter' && event.key !== ' ') {
        return;
      }
      event.preventDefault();
      onSelect();
    };

    return (
      <tr
        tabIndex={0}
        data-song-index={index}
        aria-selected={isSelected}
        onClick={onSelect}
        onKeyDown={onKeyDown}
        className={cn(
          'cursor-pointer border-b border-border/70 outline-none [&>td]:bg-background [&>td]:transition-colors hover:[&>td]:bg-accent focus-visible:[&>td]:bg-primary/15',
          (isFocused || isSelected) && '[&>td]:bg-primary/15 hover:[&>td]:bg-primary/20',
        )}
      >
        {SONG_COLUMNS.map((column) => (
          <td key={column.id} className={column.tdClassName}>
            {column.cell(song, queueStatus, bestScore)}
          </td>
        ))}
      </tr>
    );
  },
);
