import type { MicSampleFrame } from '@/types/MicSampleFrame';

export type MicSamplesCallback = (frame: MicSampleFrame) => void;
export type StopListening = () => void;

const subscribers = new Set<MicSamplesCallback>();

export const dispatchMicFrame = (frame: MicSampleFrame): void => {
  for (const callback of subscribers) {
    try {
      callback(frame);
    } catch {
      // One consumer must not break microphone delivery to other consumers.
    }
  }
};

export const subscribeMicSamples = (callback: MicSamplesCallback): StopListening => {
  subscribers.add(callback);
  return () => {
    subscribers.delete(callback);
  };
};
