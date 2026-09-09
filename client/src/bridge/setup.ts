import type { CachePaths } from '@/types/CachePaths';
import type { SetupProgress } from '@/types/SetupProgress';

import { invoke, listen } from './runtime';

export const isAppReady = async (): Promise<boolean> => {
  return await invoke<boolean>('is_ready');
};

export const triggerSetup = async (dataFolder?: string, cachePaths?: CachePaths): Promise<void> => {
  return await invoke<void>('trigger_setup', { dataPath: dataFolder, cachePaths });
};

export const onSetupProgress = async (
  cb: (progress: SetupProgress) => void,
): Promise<() => void> => {
  return await listen<SetupProgress>('setup-progress', ({ payload }) => cb(payload));
};

export const onSetupError = async (cb: (error: string) => void): Promise<() => void> => {
  return await listen<string>('setup-error', ({ payload }) => cb(payload));
};
