import { useCallback, type RefObject } from 'react';

import { SONG_SELECTOR } from './dom';

const BORDER_PADDING = 3;

export function useScrollToSong(scrollRef: RefObject<HTMLElement | null>) {
  return useCallback(
    (index: number) => {
      const container = scrollRef.current;
      if (!container) {
        return;
      }

      const cards = container.querySelectorAll<HTMLElement>(SONG_SELECTOR);

      for (const card of cards) {
        const cardIndex = Number(card.dataset.songIndex);
        if (cardIndex !== index) {
          continue;
        }

        const cardRect = card.getBoundingClientRect();
        const containerRect = container.getBoundingClientRect();
        const cardTop = cardRect.top - containerRect.top + container.scrollTop;
        const cardBottom = cardTop + cardRect.height;

        if (cardTop < container.scrollTop) {
          container.scrollTop = cardTop - BORDER_PADDING;
        } else if (cardBottom > container.scrollTop + containerRect.height) {
          container.scrollTop = cardBottom - containerRect.height + BORDER_PADDING;
        }
        break;
      }
    },
    [scrollRef],
  );
}
