import prettyBytes from 'pretty-bytes';

import type { CacheStats } from '@/types/CacheStats';

export function formatBytes(n: bigint | number) {
  return prettyBytes(n, { binary: true });
}

export function totalUsedBytes({
  songs_bytes,
  videos_bytes,
  models_bytes,
  other_bytes,
}: CacheStats): bigint {
  return songs_bytes + videos_bytes + models_bytes + other_bytes;
}

export function segmentPercent(part: bigint, total: bigint): number {
  if (total === 0n) {
    return 0;
  }

  return (Number(part) / Number(total)) * 100;
}
