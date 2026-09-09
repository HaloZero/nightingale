import { useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';

import { clearAll, clearModels, clearVideos } from '@/bridge/cache';
import { CACHE_STATS } from '@/shared/query-keys';

export const useClearCache = () => {
  const queryClient = useQueryClient();

  const clearCacheFactory = (handler: () => Promise<void>) => {
    return async () => {
      try {
        await handler();

        void queryClient.invalidateQueries({ queryKey: CACHE_STATS });

        toast.info(`Cache was successfully cleared`);
      } catch (error: unknown) {
        toast.error(
          `Error while deleting cache: ${error instanceof Error ? error.message : 'unknown error'}`,
        );
      }
    };
  };

  return {
    videos: clearCacheFactory(clearVideos),
    models: clearCacheFactory(clearModels),
    all: clearCacheFactory(clearAll),
  };
};
