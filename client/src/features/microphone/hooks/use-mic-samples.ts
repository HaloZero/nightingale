import { useEffect } from 'react';

import {
  microphoneAdapter,
  type MicSampleFrame,
  type MicrophoneAdapter,
  type StopListening,
} from '@/bridge/microphone';
import { useLatestRef } from '@/shared/hooks/use-latest-ref';

const defaultAdapter = microphoneAdapter;

/**
 * Subscribes to the raw mono PCM frame stream while `enabled` is true. The
 * `callback` ref is kept fresh between renders so consumers don't need to
 * memoize. Multiple call sites coexist; the underlying adapter fans out to
 * all subscribers from a single transport.
 */
export function useMicSamples(
  callback: (frame: MicSampleFrame) => void,
  enabled: boolean,
  adapter: MicrophoneAdapter = defaultAdapter,
): void {
  const cbRef = useLatestRef(callback);

  useEffect(() => {
    if (!enabled) {
      return undefined;
    }

    let cancelled = false;
    let stopListening: StopListening | null = null;

    adapter
      .subscribe((frame) => {
        if (!cancelled) {
          cbRef.current(frame);
        }
      })
      .then((stop) => {
        if (cancelled) {
          stop();
        } else {
          stopListening = stop;
        }
        return undefined;
      })
      .catch(() => {
        // Adapter failures are surfaced through useMicCapture; nothing to do here.
      });

    return () => {
      cancelled = true;
      stopListening?.();
    };
  }, [adapter, cbRef, enabled]);
}
