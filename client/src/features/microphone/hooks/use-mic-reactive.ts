import { useEffect, useRef, type MutableRefObject } from 'react';

import { useMicSamples } from '@/features/microphone/hooks/use-mic-samples';
import {
  type MicReactiveEvent,
  REACTIVE_EMIT_PERIOD_MS,
  REACTIVE_FFT_SIZE,
  ReactiveAnalyzer,
} from '@/features/microphone/lib/reactive-analyzer';
import { SampleRing } from '@/features/microphone/lib/sample-ring';

const MIN_PITCH_HZ = 80;
const MAX_PITCH_HZ = 1000;
const RMS_GATE = 0.012;

const RING_CAPACITY = REACTIVE_FFT_SIZE * 2;

export type { MicReactiveEvent } from '@/features/microphone/lib/reactive-analyzer';
export type MicReactiveRef = MutableRefObject<MicReactiveEvent | null>;

export function useMicReactive(enabled: boolean): MicReactiveRef {
  const ref = useRef<MicReactiveEvent | null>(null);
  const ringRef = useRef<SampleRing | null>(null);
  const sampleRateRef = useRef(0);

  if (ringRef.current === null) {
    ringRef.current = new SampleRing(RING_CAPACITY);
  }

  useMicSamples((frame) => {
    sampleRateRef.current = frame.sample_rate;
    ringRef.current?.push(frame.samples);
  }, enabled);

  useEffect(() => {
    if (!enabled) {
      ref.current = null;
      ringRef.current?.reset();
      sampleRateRef.current = 0;
      return undefined;
    }

    const analyzer = new ReactiveAnalyzer(MIN_PITCH_HZ, MAX_PITCH_HZ, RMS_GATE);
    const fftWindow = new Float32Array(REACTIVE_FFT_SIZE);

    const tick = () => {
      const ring = ringRef.current;
      const sr = sampleRateRef.current;
      if (!ring || sr === 0) {
        return;
      }
      if (!ring.readMostRecent(fftWindow)) {
        return;
      }
      ref.current = analyzer.analyze(fftWindow, sr);
    };

    const id = setInterval(tick, REACTIVE_EMIT_PERIOD_MS);

    return () => {
      clearInterval(id);
      ref.current = null;
    };
  }, [enabled]);

  return ref;
}
