import { useMenuFocus } from '@/features/menu/providers/menu-focus-context';

import type { UseMenuNavOptions } from './menu-nav/types';
import { useMenuNavInput } from './menu-nav/use-menu-nav-input';
import { useMenuNavRefs } from './menu-nav/use-menu-nav-refs';
import { useMouseMenuFocus } from './menu-nav/use-mouse-menu-focus';
import { useNavLock } from './menu-nav/use-nav-lock';
import { useScrollToSong } from './menu-nav/use-scroll-to-song';
import { useTabPanelSwitch } from './menu-nav/use-tab-panel-switch';

export function useMenuNav({ overlayOpen, onBack }: UseMenuNavOptions) {
  const menuFocus = useMenuFocus();
  const refs = useMenuNavRefs({ overlayOpen, onBack });
  const lock = useNavLock();
  const scrollToSong = useScrollToSong(menuFocus.scrollRef);

  useMouseMenuFocus({ menuFocus, refs, lock });
  useTabPanelSwitch({ menuFocus, refs, lock });
  useMenuNavInput({ menuFocus, refs, lock, scrollToSong });
}
