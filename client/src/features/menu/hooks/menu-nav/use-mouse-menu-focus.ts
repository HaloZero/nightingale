import { useEffect } from 'react';

import {
  blurActiveTextInput,
  getHoveredActionsIndex,
  getHoveredSidebarIndex,
  getHoveredSidebarSubTarget,
  getHoveredSongIndex,
} from './dom';
import type { MenuNavHookOptions } from './types';

export function useMouseMenuFocus({ menuFocus, refs, lock }: MenuNavHookOptions) {
  const { deactivate, setFocus } = menuFocus;

  useEffect(() => {
    const onMouseMove = (event: MouseEvent) => {
      if (lock.isLocked() || refs.overlayOpenRef.current) {
        return;
      }

      const target = event.target instanceof Element ? event.target : null;

      const subTarget = getHoveredSidebarSubTarget(target);
      if (subTarget) {
        blurActiveTextInput();
        setFocus((prev) => {
          if (
            prev.active &&
            prev.panel === 'sidebar' &&
            prev.sidebarIndex === subTarget.sidebarIndex &&
            prev.sidebarSubIndex === subTarget.sidebarSubIndex
          ) {
            return prev;
          }

          return {
            ...prev,
            active: true,
            panel: 'sidebar',
            sidebarIndex: subTarget.sidebarIndex,
            sidebarSubIndex: subTarget.sidebarSubIndex,
            actionsFocused: false,
            source: 'mouse',
          };
        });
        return;
      }

      const sidebarIndex = getHoveredSidebarIndex(target);
      if (sidebarIndex !== null) {
        blurActiveTextInput();
        setFocus((prev) => {
          if (prev.active && prev.panel === 'sidebar' && prev.sidebarIndex === sidebarIndex) {
            return prev;
          }

          const subReset = prev.sidebarIndex !== sidebarIndex ? { sidebarSubIndex: 0 } : null;
          return {
            ...prev,
            ...subReset,
            active: true,
            panel: 'sidebar',
            sidebarIndex,
            actionsFocused: false,
            source: 'mouse',
          };
        });
        return;
      }

      const actionsIndex = getHoveredActionsIndex(target);
      if (actionsIndex !== null) {
        blurActiveTextInput();
        setFocus((prev) => {
          if (
            prev.active &&
            prev.panel === 'songList' &&
            prev.actionsFocused &&
            prev.actionsIndex === actionsIndex
          ) {
            return prev;
          }

          return {
            ...prev,
            active: true,
            panel: 'songList',
            actionsFocused: true,
            actionsIndex,
            source: 'mouse',
          };
        });
        return;
      }

      const songIndex = getHoveredSongIndex(target);
      if (songIndex !== null) {
        blurActiveTextInput();
        setFocus((prev) => {
          if (
            prev.active &&
            prev.panel === 'songList' &&
            !prev.actionsFocused &&
            prev.songIndex === songIndex
          ) {
            return prev;
          }

          return {
            ...prev,
            active: true,
            panel: 'songList',
            songIndex,
            actionsFocused: false,
            source: 'mouse',
          };
        });
        return;
      }

      deactivate();
    };

    window.addEventListener('mousemove', onMouseMove);
    return () => window.removeEventListener('mousemove', onMouseMove);
  }, [deactivate, lock, refs, setFocus]);
}
