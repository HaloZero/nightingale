import logoUrl from '@/assets/images/logo.png';
import { SidebarHeader } from '@/shared/components/ui/sidebar';

import { ThemeToggle } from './theme-toggle';

type HeaderProps = {
  focusedSidebarIndex: number;
  registerCallback: (callback: (() => void) | null) => void;
};

export const Header = ({ focusedSidebarIndex, registerCallback }: HeaderProps) => (
  <SidebarHeader>
    <div className="relative flex w-full items-center justify-center pb-2">
      <img src={logoUrl} className="w-5/6" alt="Nightingale" />
      <ThemeToggle
        className="absolute right-1 top-0"
        focusedSidebarIndex={focusedSidebarIndex}
        registerCallback={registerCallback}
      />
    </div>
  </SidebarHeader>
);
