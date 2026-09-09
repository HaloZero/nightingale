import type { CacheStats } from '@/types/CacheStats';

import { invoke } from './runtime';

export const calculateCacheStats = async (): Promise<CacheStats> => {
  return await invoke<CacheStats>('calculate_cache_stats');
};

export const clearVideos = async (): Promise<void> => {
  return await invoke<void>('clear_videos_command');
};

export const clearModels = async (): Promise<void> => {
  return await invoke<void>('clear_models_command');
};

export const clearAll = async (): Promise<void> => {
  return await invoke<void>('clear_all');
};
