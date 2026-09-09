import { useCallback, useEffect, useLayoutEffect, useRef, type RefObject } from 'react';

import { useMenuFocus } from '@/features/menu/providers/menu-focus-context';

export type PersistentScrollSlot = 'songList' | 'sidebar';

function replaceRefValue<T>(ref: RefObject<T>, value: T): void {
  ref.current = value;
}

function scrollToTop(element: HTMLElement, top: number): void {
  element.scrollTo({ top });
}

function resetPersistedScroll(
  scrollRef: RefObject<HTMLElement | null>,
  scrollTopRef: RefObject<number>,
): void {
  scrollRef.current?.scrollTo({ top: 0 });
  replaceRefValue(scrollTopRef, 0);
}

export function usePersistentScroll(slot: PersistentScrollSlot) {
  const ctx = useMenuFocus();
  const scrollRef = slot === 'sidebar' ? ctx.sidebarScrollRef : ctx.scrollRef;
  const scrollTopRef = slot === 'sidebar' ? ctx.sidebarScrollTopRef : ctx.scrollTopRef;
  const detachRef = useRef<(() => void) | null>(null);

  const setScrollContainer = useCallback(
    (el: HTMLElement | null) => {
      detachRef.current?.();
      replaceRefValue(detachRef, null);
      replaceRefValue(scrollRef, el);

      if (!el) {
        return;
      }

      const onScroll = () => {
        replaceRefValue(scrollTopRef, el.scrollTop);
      };

      el.addEventListener('scroll', onScroll, { passive: true });
      replaceRefValue(detachRef, () => el.removeEventListener('scroll', onScroll));
    },
    [scrollRef, scrollTopRef],
  );

  // Restore on mount after layout so scrollHeight reflects the rendered list.
  // Done in a layout effect rather than inside the ref callback because the
  // callback can fire before children's final layout in some commit orderings.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    const target = scrollTopRef.current;
    if (!el || target <= 0) {
      return undefined;
    }
    scrollToTop(el, target);
    // Belt-and-suspenders: if layout shifts after this tick (e.g. async measure),
    // re-apply once more on the next frame.
    const raf = requestAnimationFrame(() => {
      if (scrollRef.current && Math.abs(scrollRef.current.scrollTop - target) > 1) {
        scrollToTop(scrollRef.current, target);
      }
    });
    return () => cancelAnimationFrame(raf);
  }, [scrollRef, scrollTopRef]);

  const resetScroll = useCallback(
    () => resetPersistedScroll(scrollRef, scrollTopRef),
    [scrollRef, scrollTopRef],
  );

  useEffect(() => () => detachRef.current?.(), []);

  return { setScrollContainer, resetScroll };
}
