import { useQuery } from '@tanstack/react-query';

import { calculateCacheStats } from '@/bridge/cache';
import { CACHE_STATS } from '@/shared/query-keys';

export const useCacheStats = () =>
  useQuery({
    queryKey: CACHE_STATS,
    queryFn: calculateCacheStats,
    refetchInterval: 5000,
  });
