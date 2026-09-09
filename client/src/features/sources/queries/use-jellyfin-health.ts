import { useQuery } from '@tanstack/react-query';

import { jellyfinPing } from '@/bridge/source';
import { getSource } from '@/features/sources/lib/library-source';
import { useConfig } from '@/shared/config/use-config';
import { JELLYFIN_HEALTH } from '@/shared/query-keys';

const ONLINE_REFRESH_MS = 30_000;
const OFFLINE_REFRESH_MS = 10_000;

/**
 * Polls `jellyfin_ping` while a Jellyfin source is configured. Backs off when
 * the server is reachable and polls faster while it's down so the user sees
 * recovery quickly. Returns `undefined` when no Jellyfin source is active.
 */
export const useJellyfinHealth = () => {
  const { data: config } = useConfig();
  const enabled = getSource(config, 'jellyfin') !== null;

  return useQuery({
    queryKey: JELLYFIN_HEALTH,
    queryFn: jellyfinPing,
    enabled,
    refetchInterval: (data) => (data?.reachable === false ? OFFLINE_REFRESH_MS : ONLINE_REFRESH_MS),
    refetchOnWindowFocus: true,
    staleTime: 5_000,
  });
};
