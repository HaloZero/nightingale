import type { ScoreRecord } from "@/types/ScoreRecord";

export function halfStarUnits(score: number): number {
  return Math.min(10, Math.round(score / 100));
}

export function topScoresForSong(
  records: ScoreRecord[],
  songHash: string,
  limit: number,
): Array<{ profile: string; score: number }> {
  const best = new Map<string, number>();

  for (const r of records) {
    if (r.song_hash !== songHash) {
      continue;
    }

    const prev = best.get(r.profile);
    if (prev === undefined || r.score > prev) {
      best.set(r.profile, r.score);
    }
  }

  return [...best.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, limit)
    .map(([profile, score]) => ({ profile, score }));
}
