import { useMemo, useRef } from 'react';

import { useLatestRef } from '@/shared/hooks/use-latest-ref';

import type { MenuNavRefs, UseMenuNavOptions } from './types';

export function useMenuNavRefs({ overlayOpen, onBack }: UseMenuNavOptions): MenuNavRefs {
  const onBackRef = useLatestRef(onBack);
  const overlayOpenRef = useLatestRef(overlayOpen);

  const lastConfirmAtRef = useRef(0);

  return useMemo(
    () => ({
      onBackRef,
      overlayOpenRef,
      lastConfirmAtRef,
    }),
    [onBackRef, overlayOpenRef],
  );
}
