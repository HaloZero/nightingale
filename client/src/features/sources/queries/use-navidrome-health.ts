import { useQuery } from '@tanstack/react-query';

import { navidromePing } from '@/bridge/source';
import { getSource } from '@/features/sources/lib/library-source';
import { useConfig } from '@/shared/config/use-config';
import { NAVIDROME_HEALTH } from '@/shared/query-keys';

const ONLINE_REFRESH_MS = 30_000;
const OFFLINE_REFRESH_MS = 10_000;

/**
 * Polls `navidrome_ping` while a Navidrome source is configured. Mirrors
 * `useJellyfinHealth`: backs off when reachable, speeds up while offline so
 * the user sees recovery quickly. Returns `undefined` when no Navidrome
 * source is active.
 */
export const useNavidromeHealth = () => {
  const { data: config } = useConfig();
  const enabled = getSource(config, 'navidrome') !== null;

  return useQuery({
    queryKey: NAVIDROME_HEALTH,
    queryFn: navidromePing,
    enabled,
    refetchInterval: (data) => (data?.reachable === false ? OFFLINE_REFRESH_MS : ONLINE_REFRESH_MS),
    refetchOnWindowFocus: true,
    staleTime: 5_000,
  });
};
