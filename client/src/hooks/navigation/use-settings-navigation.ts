import { DIALOG_FOCUSABLE_SELECTOR, useDialogNav } from "@/hooks/navigation/use-dialog-nav";
import { cn } from "@/lib/utils";
import type { RefObject } from "react";
import { useMemo } from "react";
import {
  MIC_LATENCY_MAX,
  MIC_LATENCY_STEP,
  MIC_MONITOR_GAIN_MAX,
  MIC_MONITOR_GAIN_STEP,
  NAV,
  VOCAL_THRESHOLD_MAX,
  VOCAL_THRESHOLD_MIN,
  VOCAL_THRESHOLD_STEP,
  getAnalysisNav,
  getSettingsStops,
  type SettingsTab,
} from "@/components/menu/settings/constants";

const FOCUS_RING = "relative z-10 ring-2 ring-primary";
const NO_FOCUS_RING = "focus-visible:ring-0 focus-visible:border-transparent";

function getVisibleFocusables(container: HTMLElement) {
  return Array.from(container.querySelectorAll<HTMLElement>(DIALOG_FOCUSABLE_SELECTOR)).filter(
    (el) => el.offsetWidth > 0 || el.offsetHeight > 0,
  );
}

function segmentSlotFromFlatIndex(segmentSizes: readonly number[], flatIndex: number) {
  let cursor = 0;

  for (let segment = 0; segment < segmentSizes.length; segment++) {
    const size = segmentSizes[segment] ?? 0;
    if (flatIndex < cursor + size) {
      return { segment, slot: flatIndex - cursor };
    }
    cursor += size;
  }

  return null;
}

interface UseSettingsNavigationOptions {
  containerRef: RefObject<HTMLDivElement | null>;
  tab: SettingsTab;
  isParakeet: boolean;
  showParallelAnalysis: boolean;
  showBackgroundVideos: boolean;
  micMonitorGain: number;
  micLatencySec: number;
  vocalThresholdPct: number;
  onBack: () => void;
  onTabChange: (tab: SettingsTab) => void;
  onMicMonitorGainChange: (gain: number) => void;
  onMicLatencyChange: (latencySec: number) => void;
  onVocalThresholdChange: (pct: number) => void;
}

export function useSettingsNavigation({
  containerRef,
  tab,
  isParakeet,
  showParallelAnalysis,
  showBackgroundVideos,
  micMonitorGain,
  micLatencySec,
  vocalThresholdPct,
  onBack,
  onTabChange,
  onMicMonitorGainChange,
  onMicLatencyChange,
  onVocalThresholdChange,
}: UseSettingsNavigationOptions) {
  const stops = useMemo(
    () => getSettingsStops(tab, isParakeet, showParallelAnalysis, showBackgroundVideos),
    [tab, isParakeet, showParallelAnalysis, showBackgroundVideos],
  );
  const itemCount = useMemo(() => stops.reduce((sum, size) => sum + size, 0), [stops]);
  const footerSegment = stops.length - 1;

  const { isFocused, focusSegment } = useDialogNav({
    open: true,
    itemCount,
    stops,
    onBack,
    containerRef,
    onAction: (segment, slot, action) => {
      if (segment === NAV.tabSegment && action.confirm) {
        onTabChange(slot === 0 ? "general" : "analysis");
        return true;
      }

      if (!action.left && !action.right) return false;

      if (tab === "general" && segment === NAV.general.micMonitorGain) {
        const delta = action.right ? MIC_MONITOR_GAIN_STEP : -MIC_MONITOR_GAIN_STEP;
        const next = Math.min(MIC_MONITOR_GAIN_MAX, Math.max(0, micMonitorGain + delta));
        onMicMonitorGainChange(next);
        return true;
      }

      if (tab === "general" && segment === NAV.general.micLatency) {
        const delta = action.right ? MIC_LATENCY_STEP : -MIC_LATENCY_STEP;
        const next = Math.min(MIC_LATENCY_MAX, Math.max(0, micLatencySec + delta));
        onMicLatencyChange(next);
        return true;
      }

      if (
        tab === "analysis" &&
        segment === getAnalysisNav(isParakeet, showParallelAnalysis).vocalThreshold
      ) {
        const delta = action.right ? VOCAL_THRESHOLD_STEP : -VOCAL_THRESHOLD_STEP;
        const next = Math.min(
          VOCAL_THRESHOLD_MAX,
          Math.max(VOCAL_THRESHOLD_MIN, vocalThresholdPct + delta),
        );
        onVocalThresholdChange(next);
        return true;
      }

      return false;
    },
  });

  const getFocusClassName = (segment: number, slot = 0) => {
    return cn(NO_FOCUS_RING, isFocused(segment, slot) && FOCUS_RING);
  };

  const syncFocusFromElement = (target: EventTarget | null) => {
    if (!containerRef.current || !(target instanceof Element)) {
      return;
    }

    const focusable = target.closest<HTMLElement>(DIALOG_FOCUSABLE_SELECTOR);
    if (!focusable || !containerRef.current.contains(focusable)) {
      return;
    }

    const flatIndex = getVisibleFocusables(containerRef.current).indexOf(focusable);
    if (flatIndex < 0) {
      return;
    }

    const next = segmentSlotFromFlatIndex(stops, flatIndex);
    if (next) {
      focusSegment(next.segment, next.slot);
    }
  };

  return {
    footerSegment,
    getFocusClassName,
    syncFocusFromElement,
  };
}
