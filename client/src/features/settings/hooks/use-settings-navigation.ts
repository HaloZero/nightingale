import type { RefObject } from 'react';
import { useMemo } from 'react';

import { DIALOG_FOCUSABLE_SELECTOR, useDialogNav } from '@/features/menu/hooks/use-dialog-nav';
import {
  MIC_LATENCY_MAX,
  MIC_LATENCY_STEP,
  MIC_MONITOR_GAIN_MAX,
  MIC_MONITOR_GAIN_STEP,
  NAV,
  PLAYBACK_SCALE_MAX,
  PLAYBACK_SCALE_MIN,
  PLAYBACK_SCALE_STEP,
  SETTINGS_TABS,
  VOCAL_THRESHOLD_MAX,
  VOCAL_THRESHOLD_MIN,
  VOCAL_THRESHOLD_STEP,
  getAnalysisNav,
  getSettingsStops,
  type SettingsTab,
} from '@/features/settings/components/constants';
import { cn } from '@/shared/utils/cn';

const FOCUS_RING = 'relative z-10 ring-2 ring-primary';
const NO_FOCUS_RING = 'focus-visible:ring-0 focus-visible:border-transparent';

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

type UseSettingsNavigationOptions = {
  containerRef: RefObject<HTMLDivElement | null>;
  tab: SettingsTab;
  isParakeet: boolean;
  showParallelAnalysis: boolean;
  showBackgroundVideos: boolean;
  micMonitorGain: number;
  micLatencySec: number;
  lyricsScale: number;
  pitchGraphScale: number;
  vocalThresholdPct: number;
  onBack: () => void;
  onTabChange: (tab: SettingsTab) => void;
  onMicMonitorGainChange: (gain: number) => void;
  onMicLatencyChange: (latencySec: number) => void;
  onLyricsScaleChange: (scale: number) => void;
  onPitchGraphScaleChange: (scale: number) => void;
  onVocalThresholdChange: (pct: number) => void;
};

export function useSettingsNavigation({
  containerRef,
  tab,
  isParakeet,
  showParallelAnalysis,
  showBackgroundVideos,
  micMonitorGain,
  micLatencySec,
  lyricsScale,
  pitchGraphScale,
  vocalThresholdPct,
  onBack,
  onTabChange,
  onMicMonitorGainChange,
  onMicLatencyChange,
  onLyricsScaleChange,
  onPitchGraphScaleChange,
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
      const adjustGeneral = (): boolean => {
        if (segment === NAV.general.micMonitorGain) {
          const delta = action.right ? MIC_MONITOR_GAIN_STEP : -MIC_MONITOR_GAIN_STEP;
          onMicMonitorGainChange(
            Math.min(MIC_MONITOR_GAIN_MAX, Math.max(0, micMonitorGain + delta)),
          );
          return true;
        }
        if (segment === NAV.general.micLatency) {
          const delta = action.right ? MIC_LATENCY_STEP : -MIC_LATENCY_STEP;
          onMicLatencyChange(Math.min(MIC_LATENCY_MAX, Math.max(0, micLatencySec + delta)));
          return true;
        }
        return false;
      };

      const adjustPlayback = (): boolean => {
        const delta = action.right ? PLAYBACK_SCALE_STEP : -PLAYBACK_SCALE_STEP;
        if (segment === NAV.playback.lyricsScale) {
          onLyricsScaleChange(
            Math.min(PLAYBACK_SCALE_MAX, Math.max(PLAYBACK_SCALE_MIN, lyricsScale + delta)),
          );
          return true;
        }
        if (segment === NAV.playback.pitchGraphScale) {
          onPitchGraphScaleChange(
            Math.min(PLAYBACK_SCALE_MAX, Math.max(PLAYBACK_SCALE_MIN, pitchGraphScale + delta)),
          );
          return true;
        }
        return false;
      };

      const adjustAnalysis = (): boolean => {
        if (segment !== getAnalysisNav(isParakeet, showParallelAnalysis).vocalThreshold) {
          return false;
        }
        const delta = action.right ? VOCAL_THRESHOLD_STEP : -VOCAL_THRESHOLD_STEP;
        onVocalThresholdChange(
          Math.min(VOCAL_THRESHOLD_MAX, Math.max(VOCAL_THRESHOLD_MIN, vocalThresholdPct + delta)),
        );
        return true;
      };

      if (segment === NAV.tabSegment && action.confirm) {
        onTabChange(SETTINGS_TABS[slot].value);
        return true;
      }

      if (!action.left && !action.right) {
        return false;
      }

      if (tab === 'general') {
        return adjustGeneral();
      }
      return tab === 'playback' ? adjustPlayback() : adjustAnalysis();
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
