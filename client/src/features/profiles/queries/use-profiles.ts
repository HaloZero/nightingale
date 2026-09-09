import { useQuery } from '@tanstack/react-query';

import { loadProfiles } from '@/bridge/profile';
import { PROFILES } from '@/shared/query-keys';
import type { ProfileStore } from '@/types/ProfileStore';

export const useProfiles = () =>
  useQuery<ProfileStore>({
    queryKey: PROFILES,
    queryFn: loadProfiles,
  });
