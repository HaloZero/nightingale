import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { ANALYSIS_QUEUE } from "@/queries/keys";
import { loadAnalysisQueue } from "@/bridge/songs";
import { classifyAnalysisFailure } from "@/lib/analysis-failure";

const TOAST_ID_PREFIX = "analysis-queue-failure:";

type FailureRecord = { message: string; firstSeenAt: number };

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

  useEffect(() => {
    if (!data) return;
    const entries = data.entries;

    const failuresByHash = failuresByHashRef.current;
    const stillFailingHashes = new Set<string>();

    for (const [hash, status] of Object.entries(entries)) {
      if (typeof status !== "object" || !("Failed" in status)) continue;
      stillFailingHashes.add(hash);
      const message = status.Failed;
      const existing = failuresByHash.get(hash);
      if (!existing || existing.message !== message) {
        failuresByHash.set(hash, { message, firstSeenAt: Date.now() });
      }
    }

    for (const hash of failuresByHash.keys()) {
      if (!stillFailingHashes.has(hash)) failuresByHash.delete(hash);
    }

    const categoryCounts = new Map<
      string,
      { label: string; count: number; lastFailureAt: number }
    >();
    for (const { message, firstSeenAt } of failuresByHash.values()) {
      const category = classifyAnalysisFailure(message);
      const bucket = categoryCounts.get(category.id);
      if (bucket) {
        bucket.count += 1;
        bucket.lastFailureAt = Math.max(bucket.lastFailureAt, firstSeenAt);
      } else {
        categoryCounts.set(category.id, {
          label: category.label,
          count: 1,
          lastFailureAt: firstSeenAt,
        });
      }
    }

    const nextActiveToastIds = new Set<string>();
    for (const [categoryId, { label, count, lastFailureAt }] of categoryCounts) {
      const toastId = `${TOAST_ID_PREFIX}${categoryId}`;
      nextActiveToastIds.add(toastId);
      toast.error(`${label}: ${count} song${count === 1 ? "" : "s"} failed`, {
        id: toastId,
        description: `Last failure at ${formatTime(lastFailureAt)}`,
        duration: Infinity,
      });
    }

    for (const toastId of activeToastIdsRef.current) {
      if (!nextActiveToastIds.has(toastId)) toast.dismiss(toastId);
    }
    activeToastIdsRef.current = nextActiveToastIds;
  }, [data]);
};
