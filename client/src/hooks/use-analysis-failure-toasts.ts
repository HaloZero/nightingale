import { useEffect, useRef } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ANALYSIS_QUEUE } from "@/queries/keys";
import { loadAnalysisQueue } from "@/bridge/songs";
import { acknowledgeAnalysisFailures } from "@/bridge/analysis";
import { labelForFailureKind } from "@/lib/analysis-failure";
import type { FailureKind } from "@/types/FailureKind";

const TOAST_ID_PREFIX = "analysis-queue-failure:";

const formatTime = (ms: number) =>
  new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });

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
    if (!data) return;
    const firstSeenAt = firstSeenAtRef.current;

    const failingHashes = new Set<string>();
    const unacknowledgedByKind = new Map<FailureKind, string[]>();
    const lastFailureAtByKind = new Map<FailureKind, number>();

    for (const [hash, status] of Object.entries(data.entries)) {
      if (typeof status !== "object" || !("Failed" in status)) continue;
      failingHashes.add(hash);
      if (!firstSeenAt.has(hash)) firstSeenAt.set(hash, Date.now());

      const { kind, acknowledged } = status.Failed;
      const seenAt = firstSeenAt.get(hash) as number;
      lastFailureAtByKind.set(kind, Math.max(lastFailureAtByKind.get(kind) ?? 0, seenAt));

      if (acknowledged) continue;
      const hashes = unacknowledgedByKind.get(kind);
      if (hashes) hashes.push(hash);
      else unacknowledgedByKind.set(kind, [hash]);
    }

    for (const hash of firstSeenAt.keys()) {
      if (!failingHashes.has(hash)) firstSeenAt.delete(hash);
    }

    const nextActiveToastIds = new Set<string>();
    for (const [kind, hashes] of unacknowledgedByKind) {
      const toastId = `${TOAST_ID_PREFIX}${kind}`;
      nextActiveToastIds.add(toastId);
      const count = hashes.length;

      toast.error(`${labelForFailureKind(kind)}: ${count} song${count === 1 ? "" : "s"} failed`, {
        id: toastId,
        description: `Last failure at ${formatTime(lastFailureAtByKind.get(kind) as number)}`,
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
      if (!nextActiveToastIds.has(toastId)) toast.dismiss(toastId);
    }
    activeToastIdsRef.current = nextActiveToastIds;
  }, [data, queryClient]);
};
