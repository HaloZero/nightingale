import type { ScoreRecord } from '@/types/ScoreRecord';

export function halfStarUnits(score: number): number {
  return Math.min(10, Math.round(score / 100));
}

export function topScoresForSong(
  records: ScoreRecord[],
  songHash: string,
  limit: number,
): ScoreRecord[] {
  const best = new Map<string, ScoreRecord>();

  for (const record of records) {
    if (record.song_hash !== songHash) {
      continue;
    }

    const previous = best.get(record.profile);
    if (previous === undefined || record.score > previous.score) {
      best.set(record.profile, record);
    }
  }

  return [...best.values()].toSorted((a, b) => b.score - a.score).slice(0, limit);
}
