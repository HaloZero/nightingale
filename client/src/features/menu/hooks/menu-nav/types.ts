import type { RefObject } from 'react';

import type { MenuFocusContextValue } from '@/features/menu/providers/menu-focus-context';

export type UseMenuNavOptions = {
  overlayOpen: boolean;
  onBack: () => void;
};

export type MenuNavRefs = {
  onBackRef: RefObject<() => void>;
  overlayOpenRef: RefObject<boolean>;
  lastConfirmAtRef: RefObject<number>;
};

export type MenuNavLock = {
  isLocked: () => boolean;
  lockTemporarily: () => void;
};

export type MenuNavHookOptions = {
  menuFocus: MenuFocusContextValue;
  refs: MenuNavRefs;
  lock: MenuNavLock;
};
