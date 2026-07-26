import { useProfiles } from "@/queries/use-profiles";
import { useMemo } from "react";

export function useBestScoresBySongForActiveProfile(): Map<string, number> {
  const { data } = useProfiles();
  const active = data?.active;
  const scores = data?.scores ?? [];

  return useMemo(() => {
    const map = new Map<string, number>();
    if (!active) {
      return map;
    }

    for (const r of scores) {
      if (r.profile !== active) {
        continue;
      }

      const prev = map.get(r.song_hash);
      if (prev === undefined || r.score > prev) {
        map.set(r.song_hash, r.score);
      }
    }

    return map;
  }, [scores, active]);
}
