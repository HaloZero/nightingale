import type { MicCaptureOptions } from '@/types/MicCaptureOptions';
import type { MicrophoneInfo } from '@/types/MicrophoneInfo';
import type { MicSampleFrame } from '@/types/MicSampleFrame';

import type { MicSamplesCallback, StopListening } from './microphone-samples';
import { tauriMicrophoneAdapter } from './microphone.tauri';
import { setWebMicMonitorGain, webMicrophoneAdapter } from './microphone.web';
import { isTauri } from './runtime';

export type { MicSamplesCallback, StopListening } from './microphone-samples';
export type { MicCaptureOptions, MicSampleFrame };

export type MicrophoneAdapter = {
  listDevices(): Promise<MicrophoneInfo[]>;
  startCapture(preferred: string | null, options: MicCaptureOptions): Promise<string>;
  stopCapture(): Promise<void>;
  subscribe(cb: MicSamplesCallback): Promise<StopListening>;
};

export { tauriMicrophoneAdapter, webMicrophoneAdapter };

export const microphoneAdapter: MicrophoneAdapter = isTauri
  ? tauriMicrophoneAdapter
  : webMicrophoneAdapter;

export const listMicrophones = (): Promise<MicrophoneInfo[]> => microphoneAdapter.listDevices();

export const startMicCapture = (
  preferred: string | null,
  options: MicCaptureOptions,
): Promise<string> => microphoneAdapter.startCapture(preferred, options);

export const stopMicCapture = (): Promise<void> => microphoneAdapter.stopCapture();

/**
 * Pushes a new monitor gain to the active web capture. In Tauri the same value
 * is applied server-side from `save_config` via `set_monitor_gain`, so this
 * call is a no-op in that build.
 */
export const updateMicMonitorGain = (value: number): void => {
  if (isTauri) {
    return;
  }
  setWebMicMonitorGain(value);
};
