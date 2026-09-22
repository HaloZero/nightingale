import { useEffect, useState } from 'react';

import { isTauri } from '@/bridge/runtime';

const MOBILE_QUERY = '(max-width: 767px)';

export function isMobileViewport(): boolean {
  return typeof window !== 'undefined' && window.matchMedia(MOBILE_QUERY).matches;
}

export function useIsMobile(): boolean {
  const [isMobile, setIsMobile] = useState(isMobileViewport);

  useEffect(() => {
    const media = window.matchMedia(MOBILE_QUERY);
    const sync = () => setIsMobile(media.matches);

    sync();
    media.addEventListener('change', sync);
    return () => media.removeEventListener('change', sync);
  }, []);

  return isMobile;
}

/** Narrower than `useIsMobile`: excludes the Tauri desktop app so a shrunk
 * desktop window doesn't get treated as a phone browser. Callers use this to
 * skip mobile-web-only work (e.g. decorative background video). */
export function useIsMobileWeb(): boolean {
  const isMobile = useIsMobile();
  return isMobile && !isTauri;
}
