import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef } from 'react';
import { toast } from 'sonner';

import { acknowledgeAnalysisFailures } from '@/bridge/analysis';
import { loadAnalysisQueue } from '@/bridge/songs';
import { labelForFailureKind } from '@/features/playback/lib/analysis-failure';
import { ANALYSIS_QUEUE } from '@/shared/query-keys';
import type { AnalysisQueue } from '@/types/AnalysisQueue';
import type { FailureKind } from '@/types/FailureKind';

const TOAST_ID_PREFIX = 'analysis-queue-failure:';

const formatTime = (ms: number) =>
  new Date(ms).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });

type FailureAccumulation = {
  failingHashes: Set<string>;
  unacknowledgedByKind: Map<FailureKind, string[]>;
  lastFailureAtByKind: Map<FailureKind, number>;
};

// One failing hash's contribution to `accumulateFailures`'s running totals
// -- split out so the loop over every queue entry stays under the
// complexity limit on its own.
type FailingHashEntry = { hash: string; kind: FailureKind; acknowledged: boolean };

function recordFailingHash(
  entry: FailingHashEntry,
  firstSeenAt: Map<string, number>,
  acc: Omit<FailureAccumulation, 'failingHashes'>,
): void {
  const { hash, kind, acknowledged } = entry;
  if (!firstSeenAt.has(hash)) {
    firstSeenAt.set(hash, Date.now());
  }
  const seenAt = firstSeenAt.get(hash) ?? Date.now();
  acc.lastFailureAtByKind.set(kind, Math.max(acc.lastFailureAtByKind.get(kind) ?? 0, seenAt));

  if (acknowledged) {
    return;
  }
  const hashes = acc.unacknowledgedByKind.get(kind);
  if (hashes) {
    hashes.push(hash);
  } else {
    acc.unacknowledgedByKind.set(kind, [hash]);
  }
}

// Walks the queue once, splitting each failing hash's `firstSeenAt` bookkeeping
// (updated in place -- purely for the toast's "last failure at" text) from the
// per-kind grouping used to decide which toasts to show.
function accumulateFailures(
  data: AnalysisQueue,
  firstSeenAt: Map<string, number>,
): FailureAccumulation {
  const failingHashes = new Set<string>();
  const unacknowledgedByKind = new Map<FailureKind, string[]>();
  const lastFailureAtByKind = new Map<FailureKind, number>();

  for (const [hash, status] of Object.entries(data.entries)) {
    if (typeof status !== 'object' || !('Failed' in status)) {
      continue;
    }
    failingHashes.add(hash);
    recordFailingHash({ hash, ...status.Failed }, firstSeenAt, {
      unacknowledgedByKind,
      lastFailureAtByKind,
    });
  }

  for (const hash of firstSeenAt.keys()) {
    if (!failingHashes.has(hash)) {
      firstSeenAt.delete(hash);
    }
  }

  return { failingHashes, unacknowledgedByKind, lastFailureAtByKind };
}

export const useAnalysisFailureToasts = () => {
  const queryClient = useQueryClient();
  const { data } = useQuery({
    queryKey: ANALYSIS_QUEUE,
    queryFn: loadAnalysisQueue,
    refetchInterval: 2500,
  });

  // First-seen time per failing hash, purely for the toast's "last failure
  // at" text -- whether a toast shows is driven by the backend's
  // `acknowledged` flag, not this.
  const firstSeenAtRef = useRef(new Map<string, number>());
  const activeToastIdsRef = useRef(new Set<string>());

  useEffect(() => {
    if (!data) {
      return;
    }
    const firstSeenAt = firstSeenAtRef.current;
    const { unacknowledgedByKind, lastFailureAtByKind } = accumulateFailures(data, firstSeenAt);

    const nextActiveToastIds = new Set<string>();
    for (const [kind, hashes] of unacknowledgedByKind) {
      const toastId = `${TOAST_ID_PREFIX}${kind}`;
      nextActiveToastIds.add(toastId);
      const count = hashes.length;

      toast.error(`${labelForFailureKind(kind)}: ${count} song${count === 1 ? '' : 's'} failed`, {
        id: toastId,
        description: `Last failure at ${formatTime(lastFailureAtByKind.get(kind) ?? Date.now())}`,
        duration: Infinity,
        closeButton: true,
        onDismiss: () => {
          void acknowledgeAnalysisFailures(kind, hashes).then(() =>
            queryClient.invalidateQueries({ queryKey: ANALYSIS_QUEUE }),
          );
        },
      });
    }

    for (const toastId of activeToastIdsRef.current) {
      if (!nextActiveToastIds.has(toastId)) {
        toast.dismiss(toastId);
      }
    }
    activeToastIdsRef.current = nextActiveToastIds;
  }, [data, queryClient]);
};
