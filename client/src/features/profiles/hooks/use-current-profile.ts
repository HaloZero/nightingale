import { useProfiles } from '@/features/profiles/queries/use-profiles';

export const useCurrentProfile = (): string | undefined => {
  const { data } = useProfiles();

  if (!data) {
    return undefined;
  }

  const { active, profiles } = data;

  return profiles.find((name) => name === active);
};
