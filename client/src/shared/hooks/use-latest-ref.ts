import { useLayoutEffect, useRef } from 'react';
import type { MutableRefObject } from 'react';

/**
 * Mirrors the latest render's `value` into a ref so closures captured by
 * effects, event handlers, or long-lived listeners can read the current value
 * without being recreated whenever it changes.
 */
export function useLatestRef<T>(value: T): MutableRefObject<T> {
  const ref = useRef(value);

  useLayoutEffect(() => {
    ref.current = value;
  });

  return ref;
}
