import { useQuery } from '@tanstack/react-query';

import { getPreloadedConfig, loadConfig } from '@/bridge/config';
import { CONFIG } from '@/shared/query-keys';

export const useConfig = () => {
  const preloaded = getPreloadedConfig();

  return useQuery({
    queryKey: CONFIG,
    queryFn: loadConfig,
    ...(preloaded !== undefined ? { initialData: preloaded } : {}),
  });
};
