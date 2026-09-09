import { useContext, useEffect } from 'react';

import { NavInputContext, type NavAction } from '@/app/providers/nav-input-context';
import { useLatestRef } from '@/shared/hooks/use-latest-ref';

export type { NavAction };

export function useNavInput(handler: (action: NavAction) => void) {
  const ctx = useContext(NavInputContext);
  const handlerRef = useLatestRef(handler);

  useEffect(() => {
    if (!ctx) {
      return undefined;
    }
    return ctx.subscribe((action) => handlerRef.current(action));
  }, [ctx, handlerRef]);
}
