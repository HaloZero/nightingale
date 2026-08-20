import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { ANALYSIS_QUEUE } from "@/queries/keys";
import { loadAnalysisQueue } from "@/bridge/songs";
import { labelForFailureKind } from "@/lib/analysis-failure";
import type { FailureKind } from "@/types/FailureKind";

const TOAST_ID_PREFIX = "analysis-queue-failure:";

type FailureRecord = { kind: FailureKind; firstSeenAt: number };

const formatTime = (ms: number) =>
  new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });

// Keyed by file hash so a song that clears its failure (requeued, removed,
// or fixed) drops out without needing the queue to tell us why it left.
export const useAnalysisFailureToasts = () => {
  const { data } = useQuery({
    queryKey: ANALYSIS_QUEUE,
    queryFn: loadAnalysisQueue,
    refetchInterval: 2500,
  });

  const failuresByHashRef = useRef(new Map<string, FailureRecord>());
  const activeToastIdsRef = useRef(new Set<string>());
  // What we last *toasted* per category (not just computed) -- lets a
  // manually-dismissed toast stay dismissed until the underlying failure
  // count/timestamp actually moves, instead of popping back on every poll.
  const lastToastedRef = useRef(new Map<FailureKind, { count: number; lastFailureAt: number }>());

  useEffect(() => {
    if (!data) return;
    const entries = data.entries;

    const failuresByHash = failuresByHashRef.current;
    const stillFailingHashes = new Set<string>();

    for (const [hash, status] of Object.entries(entries)) {
      if (typeof status !== "object" || !("Failed" in status)) continue;
      stillFailingHashes.add(hash);
      const { kind } = status.Failed;
      const existing = failuresByHash.get(hash);
      if (!existing || existing.kind !== kind) {
        failuresByHash.set(hash, { kind, firstSeenAt: Date.now() });
      }
    }

    for (const hash of failuresByHash.keys()) {
      if (!stillFailingHashes.has(hash)) failuresByHash.delete(hash);
    }

    const categoryCounts = new Map<FailureKind, { count: number; lastFailureAt: number }>();
    for (const { kind, firstSeenAt } of failuresByHash.values()) {
      const bucket = categoryCounts.get(kind);
      if (bucket) {
        bucket.count += 1;
        bucket.lastFailureAt = Math.max(bucket.lastFailureAt, firstSeenAt);
      } else {
        categoryCounts.set(kind, { count: 1, lastFailureAt: firstSeenAt });
      }
    }

    const lastToasted = lastToastedRef.current;
    const nextActiveToastIds = new Set<string>();
    for (const [kind, { count, lastFailureAt }] of categoryCounts) {
      const toastId = `${TOAST_ID_PREFIX}${kind}`;
      nextActiveToastIds.add(toastId);

      const previous = lastToasted.get(kind);
      if (previous && previous.count === count && previous.lastFailureAt === lastFailureAt) {
        // Nothing new since we last showed this category's toast -- if the
        // user dismissed it, leave it dismissed rather than re-raising it.
        continue;
      }
      lastToasted.set(kind, { count, lastFailureAt });

      toast.error(`${labelForFailureKind(kind)}: ${count} song${count === 1 ? "" : "s"} failed`, {
        id: toastId,
        description: `Last failure at ${formatTime(lastFailureAt)}`,
        duration: Infinity,
      });
    }

    for (const toastId of activeToastIdsRef.current) {
      if (!nextActiveToastIds.has(toastId)) toast.dismiss(toastId);
    }
    activeToastIdsRef.current = nextActiveToastIds;

    for (const kind of lastToasted.keys()) {
      if (!categoryCounts.has(kind)) lastToasted.delete(kind);
    }
  }, [data]);
};
