import { useCallback } from 'react';

import type { NavAction } from '@/app/providers/nav-input-context';
import type { MenuFocus } from '@/features/menu/providers/menu-focus-context';

import { useNavInput } from '../use-nav-input';
import {
  blurActiveTextInput,
  getActionsCount,
  getActionTarget,
  getSongGridTarget,
  isSongGrid,
  type SongGridDirection,
} from './dom';
import type { MenuNavHookOptions } from './types';

const CONFIRM_COOLDOWN_MS = 140;

type UseMenuNavInputOptions = {
  scrollToSong: (index: number) => void;
} & MenuNavHookOptions;

function hasInput(action: NavAction): boolean {
  return action.up || action.down || action.left || action.right || action.confirm || action.back;
}

export function useMenuNavInput({ menuFocus, refs, lock, scrollToSong }: UseMenuNavInputOptions) {
  const { activate, actionsRef } = menuFocus;

  useNavInput(
    useCallback(
      (action) => {
        const handleBack = (): boolean => {
          if (!action.back) {
            return false;
          }
          const handled = actionsRef.current.onSidebarBack?.();
          if (handled !== true) {
            refs.onBackRef.current();
          }
          return true;
        };

        const handleDirectionalInput = (): void => {
          if (handleSongGridAction(action, menuFocus, scrollToSong)) {
            return;
          }
          if ((action.left || action.right) && handleActionsHorizontal(action, menuFocus)) {
            return;
          }
          if ((action.left || action.right) && handleHorizontalAction(action, menuFocus)) {
            return;
          }
          if (action.up || action.down) {
            handleVerticalAction(action, menuFocus, scrollToSong);
          }
        };

        const ignoreInput =
          refs.overlayOpenRef.current ||
          !hasInput(action) ||
          menuFocus.focus.panel === 'songDetails';
        if (ignoreInput || handleBack()) {
          return;
        }

        if (actionsRef.current.isSidebarBusy?.() === true) {
          return;
        }

        blurActiveTextInput();
        activate();
        lock.lockTemporarily();

        if (action.confirm) {
          handleConfirmAction(menuFocus, refs);
          return;
        }

        handleDirectionalInput();
      },
      [actionsRef, activate, lock, menuFocus, refs, scrollToSong],
    ),
  );
}

function confirmFocusedAction(
  actionsRef: MenuNavHookOptions['menuFocus']['actionsRef'],
  index: number,
): void {
  if (actionsRef.current.onConfirmActions?.(index) !== true) {
    getActionTarget(index)?.click();
  }
}

function handleConfirmAction(
  { actionsRef, focus }: Pick<MenuNavHookOptions['menuFocus'], 'actionsRef' | 'focus'>,
  refs: MenuNavHookOptions['refs'],
) {
  const now = performance.now();
  if (!focus.active) {
    return;
  }
  if (now - refs.lastConfirmAtRef.current < CONFIRM_COOLDOWN_MS) {
    return;
  }
  refs.lastConfirmAtRef.current = now;

  if (focus.panel === 'songList') {
    if (focus.actionsFocused) {
      confirmFocusedAction(actionsRef, focus.actionsIndex);
    } else {
      actionsRef.current.onConfirmSong?.(focus.songIndex);
    }
    return;
  }

  if (focus.panel === 'sidebar') {
    actionsRef.current.onConfirmSidebar?.(focus.sidebarIndex);
  }
}

function getGridDirection(action: NavAction): SongGridDirection | null {
  if (action.up) {
    return 'up';
  }
  if (action.down) {
    return 'down';
  }
  if (action.left) {
    return 'left';
  }
  if (action.right) {
    return 'right';
  }
  return null;
}

function handleSongGridAction(
  action: NavAction,
  {
    focus,
    actionsRef,
    scrollRef,
    setFocus,
  }: Pick<MenuNavHookOptions['menuFocus'], 'focus' | 'actionsRef' | 'scrollRef' | 'setFocus'>,
  scrollToSong: (index: number) => void,
): boolean {
  const direction = getGridDirection(action);
  const container = scrollRef.current;
  if (!direction || focus.panel !== 'songList' || focus.actionsFocused || !isSongGrid(container)) {
    return false;
  }

  const targetIndex = getSongGridTarget(container, focus.songIndex, direction);
  if (targetIndex !== null) {
    setFocus((previous) => ({
      ...previous,
      active: true,
      songIndex: targetIndex,
      actionsFocused: false,
      source: 'nav',
    }));
    scrollToSong(targetIndex);
    return true;
  }

  setFocus((previous) => {
    let panel = previous.panel;
    let actionsFocused = false;

    if (direction === 'left') {
      panel = 'sidebar';
    } else if (direction === 'right' && actionsRef.current.hasSongDetails) {
      panel = 'songDetails';
    } else if (direction === 'up') {
      actionsFocused = true;
    }

    return {
      ...previous,
      active: true,
      panel,
      actionsFocused,
      source: 'nav',
    };
  });
  return true;
}

function handleActionsHorizontal(
  action: NavAction,
  {
    focus,
    actionsRef,
    setFocus,
  }: Pick<MenuNavHookOptions['menuFocus'], 'focus' | 'actionsRef' | 'setFocus'>,
): boolean {
  if (focus.panel !== 'songList' || !focus.actionsFocused) {
    return false;
  }

  const lastIndex = Math.max(0, getActionsCount() - 1);
  setFocus((previous) => {
    if (action.left && previous.actionsIndex > 0) {
      return { ...previous, actionsIndex: previous.actionsIndex - 1, source: 'nav' };
    }
    if (action.right && previous.actionsIndex < lastIndex) {
      return { ...previous, actionsIndex: previous.actionsIndex + 1, source: 'nav' };
    }

    let panel = previous.panel;
    if (action.left) {
      panel = 'sidebar';
    } else if (actionsRef.current.hasSongDetails) {
      panel = 'songDetails';
    }

    return {
      ...previous,
      panel,
      actionsFocused: false,
      source: 'nav',
    };
  });
  return true;
}

function handleHorizontalAction(
  action: NavAction,
  {
    focus,
    actionsRef,
    setFocus,
  }: Pick<MenuNavHookOptions['menuFocus'], 'focus' | 'actionsRef' | 'setFocus'>,
): boolean {
  if (focus.panel === 'sidebar') {
    const subCount = actionsRef.current.sidebarSubCountByIndex.get(focus.sidebarIndex);
    if (typeof subCount === 'number' && subCount > 1) {
      const delta = action.left ? -1 : 1;
      const nextSub = Math.max(0, Math.min(subCount - 1, focus.sidebarSubIndex + delta));
      if (nextSub !== focus.sidebarSubIndex) {
        setFocus((previous) => ({
          ...previous,
          sidebarSubIndex: nextSub,
          active: true,
          source: 'nav',
        }));
        return true;
      }
      if (action.left) {
        return true;
      }
    }
  }

  if (action.left) {
    setFocus((prev) => ({
      ...prev,
      panel: prev.panel === 'songList' ? 'sidebar' : prev.panel,
      actionsFocused: false,
      active: true,
      source: 'nav',
    }));
    return true;
  }

  setFocus((prev) => {
    let panel = prev.panel;
    if (prev.panel === 'sidebar') {
      panel = 'songList';
    } else if (prev.panel === 'songList' && actionsRef.current.hasSongDetails) {
      panel = 'songDetails';
    }

    return {
      ...prev,
      panel,
      actionsFocused: false,
      active: true,
      source: 'nav',
    };
  });
  return true;
}

type SongListMove = {
  next: MenuFocus;
  previous: MenuFocus;
  action: NavAction;
  songCount: number;
  scrollToSong: (index: number) => void;
};

function moveSongListFocus(move: SongListMove): void {
  if (move.previous.actionsFocused) {
    if (move.action.down) {
      move.next.actionsFocused = false;
      move.next.songIndex = 0;
      move.scrollToSong(0);
    }
    return;
  }
  if (move.action.up) {
    move.next.actionsFocused = move.previous.songIndex <= 0;
    move.next.songIndex = Math.max(0, move.previous.songIndex - 1);
    move.scrollToSong(move.next.songIndex);
    return;
  }
  if (move.action.down && move.previous.songIndex < move.songCount - 1) {
    move.next.songIndex = move.previous.songIndex + 1;
    move.scrollToSong(move.next.songIndex);
  }
}

function moveSidebarFocus(
  next: MenuFocus,
  previous: MenuFocus,
  action: NavAction,
  sidebarCount: number,
): void {
  if (sidebarCount <= 0) {
    next.sidebarIndex = 0;
    next.sidebarSubIndex = 0;
    return;
  }
  if (action.up) {
    next.sidebarIndex = Math.max(0, previous.sidebarIndex - 1);
  } else if (action.down) {
    next.sidebarIndex = Math.min(sidebarCount - 1, previous.sidebarIndex + 1);
  }
  next.sidebarSubIndex = 0;
}

function handleVerticalAction(
  action: NavAction,
  { actionsRef, setFocus }: Pick<MenuNavHookOptions['menuFocus'], 'actionsRef' | 'setFocus'>,
  scrollToSong: (index: number) => void,
) {
  setFocus((prev) => {
    const next = { ...prev, active: true, source: 'nav' as const };

    if (prev.panel === 'songList') {
      moveSongListFocus({
        next,
        previous: prev,
        action,
        songCount: actionsRef.current.songCount,
        scrollToSong,
      });
    } else if (prev.panel === 'sidebar') {
      moveSidebarFocus(next, prev, action, actionsRef.current.sidebarCount);
    }

    return next;
  });
}
