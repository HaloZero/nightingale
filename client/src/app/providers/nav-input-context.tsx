import { createContext, useCallback, useEffect, useMemo, useRef, type ReactNode } from 'react';

import { useGamepad, type GamepadSnapshot } from '@/features/menu/hooks/use-gamepad';

export type NavAction = {
  up: boolean;
  down: boolean;
  left: boolean;
  right: boolean;
  confirm: boolean;
  back: boolean;
};

type NavSubscriber = (action: NavAction) => void;
type Unsubscribe = () => void;

export type NavInputContextValue = {
  subscribe: (fn: NavSubscriber) => Unsubscribe;
};

export const NavInputContext = createContext<NavInputContextValue | null>(null);

const IGNORED_ELEMENTS = new Set(['INPUT', 'TEXTAREA', 'SELECT']);

function isTextInput(el: Element | null): boolean {
  if (!el) {
    return false;
  }
  if (IGNORED_ELEMENTS.has(el.tagName)) {
    return true;
  }
  if (el instanceof HTMLElement && el.isContentEditable) {
    return true;
  }
  return false;
}

function isInsideManagedWidget(el: Element | null): boolean {
  if (!el) {
    return false;
  }
  return el.closest('[role="menu"], [role="listbox"], [data-nav-passthrough]') !== null;
}

function blurActiveElement() {
  const el = document.activeElement;
  if (el instanceof HTMLElement && !isTextInput(el)) {
    el.blur();
  }
}

function keyToAction(key: string): Partial<NavAction> | null {
  switch (key) {
    case 'ArrowUp':
      return { up: true };
    case 'ArrowDown':
      return { down: true };
    case 'ArrowLeft':
      return { left: true };
    case 'ArrowRight':
      return { right: true };
    case 'Enter':
      return { confirm: true };
    case 'Escape':
      return { back: true };
    default:
      return null;
  }
}

export function NavInputProvider({ children }: { children: ReactNode }) {
  const subscribersRef = useRef(new Set<NavSubscriber>());

  const fire = useCallback((partial: Partial<NavAction>) => {
    const action: NavAction = {
      up: false,
      down: false,
      left: false,
      right: false,
      confirm: false,
      back: false,
      ...partial,
    };
    for (const fn of subscribersRef.current) {
      fn(action);
    }
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (isTextInput(document.activeElement)) {
        return;
      }
      if (isInsideManagedWidget(document.activeElement)) {
        return;
      }

      const partial = keyToAction(e.key);
      if (!partial) {
        return;
      }

      e.preventDefault();
      e.stopPropagation();
      if (!e.repeat) {
        blurActiveElement();
      }

      fire(partial);
    };

    window.addEventListener('keydown', onKeyDown, true);
    return () => window.removeEventListener('keydown', onKeyDown, true);
  }, [fire]);

  const onGamepad = useCallback(
    (snap: GamepadSnapshot) => {
      const action: NavAction = {
        up: snap.up,
        down: snap.down,
        left: snap.left,
        right: snap.right,
        confirm: snap.confirm,
        back: snap.back,
      };

      const hasEdge =
        action.up || action.down || action.left || action.right || action.confirm || action.back;

      if (hasEdge) {
        fire(action);
      }
    },
    [fire],
  );

  useGamepad(onGamepad);

  const subscribe = useCallback((fn: NavSubscriber): Unsubscribe => {
    subscribersRef.current.add(fn);
    return () => {
      subscribersRef.current.delete(fn);
    };
  }, []);
  const value = useMemo<NavInputContextValue>(() => ({ subscribe }), [subscribe]);

  return <NavInputContext.Provider value={value}>{children}</NavInputContext.Provider>;
}
