import { useCallback } from "react";
import type { NavAction } from "@/contexts/nav-input-context";
import { useNavInput } from "../use-nav-input";
import { blurActiveTextInput, getSongGridTarget, isSongGrid, type SongGridDirection } from "./dom";
import type { MenuNavHookOptions } from "./types";

const CONFIRM_COOLDOWN_MS = 140;

interface UseMenuNavInputOptions extends MenuNavHookOptions {
  scrollToSong: (index: number) => void;
}

function hasInput(action: NavAction): boolean {
  return action.up || action.down || action.left || action.right || action.confirm || action.back;
}

export function useMenuNavInput({ menuFocus, refs, lock, scrollToSong }: UseMenuNavInputOptions) {
  const { activate, actionsRef } = menuFocus;

  useNavInput(
    useCallback(
      (action) => {
        if (
          refs.overlayOpenRef.current ||
          !hasInput(action) ||
          menuFocus.focus.panel === "songDetails"
        ) {
          return;
        }

        if (action.back) {
          const handled = actionsRef.current.onSidebarBack?.();
          if (!handled) {
            refs.onBackRef.current();
          }
          return;
        }

        if (actionsRef.current.isSidebarBusy?.()) {
          return;
        }

        blurActiveTextInput();
        activate();
        lock.lockTemporarily();

        if (action.confirm) {
          handleConfirmAction(menuFocus, refs);
          return;
        }

        if (
          (action.up || action.down || action.left || action.right) &&
          handleSongGridAction(action, menuFocus, scrollToSong)
        ) {
          return;
        }

        if (action.left || action.right) {
          if (handleHorizontalAction(action, menuFocus)) {
            return;
          }
        }

        if (action.up || action.down) {
          handleVerticalAction(action, menuFocus, scrollToSong);
        }
      },
      [actionsRef, activate, lock, menuFocus, refs, scrollToSong],
    ),
  );
}

function handleConfirmAction(
  { actionsRef, focus }: Pick<MenuNavHookOptions["menuFocus"], "actionsRef" | "focus">,
  refs: MenuNavHookOptions["refs"],
) {
  const now = performance.now();
  if (!focus.active) return;
  if (now - refs.lastConfirmAtRef.current < CONFIRM_COOLDOWN_MS) return;
  refs.lastConfirmAtRef.current = now;

  if (focus.panel === "songList") {
    if (focus.actionsFocused) actionsRef.current.onConfirmActions?.();
    else actionsRef.current.onConfirmSong?.(focus.songIndex);
    return;
  }

  if (focus.panel === "sidebar") {
    actionsRef.current.onConfirmSidebar?.(focus.sidebarIndex);
  }
}

function getGridDirection(action: NavAction): SongGridDirection | null {
  if (action.up) return "up";
  if (action.down) return "down";
  if (action.left) return "left";
  if (action.right) return "right";
  return null;
}

function handleSongGridAction(
  action: NavAction,
  {
    focus,
    actionsRef,
    scrollRef,
    setFocus,
  }: Pick<MenuNavHookOptions["menuFocus"], "focus" | "actionsRef" | "scrollRef" | "setFocus">,
  scrollToSong: (index: number) => void,
): boolean {
  const direction = getGridDirection(action);
  const container = scrollRef.current;
  if (!direction || focus.panel !== "songList" || focus.actionsFocused || !isSongGrid(container)) {
    return false;
  }

  const targetIndex = getSongGridTarget(container, focus.songIndex, direction);
  if (targetIndex !== null) {
    setFocus((previous) => ({
      ...previous,
      active: true,
      songIndex: targetIndex,
      actionsFocused: false,
      source: "nav",
    }));
    scrollToSong(targetIndex);
    return true;
  }

  setFocus((previous) => {
    let panel = previous.panel;
    let actionsFocused = false;

    if (direction === "left") panel = "sidebar";
    else if (direction === "right" && actionsRef.current.hasSongDetails) panel = "songDetails";
    else if (direction === "up") actionsFocused = true;

    return {
      ...previous,
      active: true,
      panel,
      actionsFocused,
      source: "nav",
    };
  });
  return true;
}

function handleHorizontalAction(
  action: NavAction,
  { actionsRef, setFocus }: Pick<MenuNavHookOptions["menuFocus"], "actionsRef" | "setFocus">,
): boolean {
  let handled = false;

  setFocus((prev) => {
    if (prev.panel === "sidebar") {
      const subCount = actionsRef.current.sidebarSubCountByIndex.get(prev.sidebarIndex);
      if (subCount && subCount > 1) {
        const delta = action.left ? -1 : 1;
        const nextSub = Math.max(0, Math.min(subCount - 1, prev.sidebarSubIndex + delta));
        if (nextSub === prev.sidebarSubIndex) {
          handled = action.left;
          return prev;
        }

        handled = true;
        return { ...prev, sidebarSubIndex: nextSub, active: true, source: "nav" };
      }
    }

    return prev;
  });

  if (handled) {
    return true;
  }

  if (action.left) {
    setFocus((prev) => ({
      ...prev,
      panel: prev.panel === "songList" ? "sidebar" : prev.panel,
      actionsFocused: false,
      active: true,
      source: "nav",
    }));
    return true;
  }

  setFocus((prev) => {
    let panel = prev.panel;
    if (prev.panel === "sidebar") panel = "songList";
    else if (prev.panel === "songList" && actionsRef.current.hasSongDetails) {
      panel = "songDetails";
    }

    return {
      ...prev,
      panel,
      actionsFocused: false,
      active: true,
      source: "nav",
    };
  });
  return true;
}

function handleVerticalAction(
  action: NavAction,
  { actionsRef, setFocus }: Pick<MenuNavHookOptions["menuFocus"], "actionsRef" | "setFocus">,
  scrollToSong: (index: number) => void,
) {
  setFocus((prev) => {
    const next = { ...prev, active: true, source: "nav" as const };

    if (prev.panel === "songList") {
      const songCount = actionsRef.current.songCount;

      if (prev.actionsFocused) {
        if (action.down) {
          next.actionsFocused = false;
          next.songIndex = 0;
          scrollToSong(0);
        }
      } else if (action.up) {
        if (prev.songIndex <= 0) {
          next.actionsFocused = true;
        } else {
          next.songIndex = prev.songIndex - 1;
          scrollToSong(prev.songIndex - 1);
        }
      } else if (action.down) {
        if (prev.songIndex < songCount - 1) {
          next.songIndex = prev.songIndex + 1;
          scrollToSong(prev.songIndex + 1);
        }
      }
    } else if (prev.panel === "sidebar") {
      const sidebarCount = actionsRef.current.sidebarCount;
      if (sidebarCount <= 0) {
        next.sidebarIndex = 0;
        next.sidebarSubIndex = 0;
        return next;
      }

      if (action.up) {
        next.sidebarIndex = Math.max(0, prev.sidebarIndex - 1);
        next.sidebarSubIndex = 0;
      } else if (action.down) {
        next.sidebarIndex = Math.min(sidebarCount - 1, prev.sidebarIndex + 1);
        next.sidebarSubIndex = 0;
      }
    }

    return next;
  });
}
