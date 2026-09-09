import { useCallback, useEffect, useMemo, useRef } from 'react';

import type { MenuNavLock } from './types';

const NAV_LOCK_MS = 120;

export function useNavLock(): MenuNavLock {
  const lockedRef = useRef(false);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const isLocked = useCallback(() => lockedRef.current, []);

  const lockTemporarily = useCallback(() => {
    lockedRef.current = true;
    clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      lockedRef.current = false;
    }, NAV_LOCK_MS);
  }, []);

  useEffect(() => {
    return () => {
      clearTimeout(timerRef.current);
    };
  }, []);

  return useMemo(() => ({ isLocked, lockTemporarily }), [isLocked, lockTemporarily]);
}
