import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from 'react';

import {
  playbackAdapter,
  fetchPixabayVideos,
  onPixabayVideoDownloaded,
  type PixabayVideoDownloaded,
} from '@/bridge/playback';
import { PixabayFlavorLibrary } from '@/features/playback/lib/pixabay-flavor-library';
import {
  getNextFlavor,
  isVideoFlavor,
  type VideoFlavor,
} from '@/features/playback/lib/video-flavor';
import { useLatestRef } from '@/shared/hooks/use-latest-ref';

const SLOT_COUNT = 3;
const STALL_TIMEOUT_MS = 4000;
const MIN_QUEUE_BEFORE_REFRESH = 2;
const REFRESH_COOLDOWN_MS = 8000;
const NEAR_END_TOLERANCE_SEC = 0.05;

export type PixabaySlot = {
  id: 'primary' | 'secondary';
  ref: RefObject<HTMLVideoElement | null>;
  src: string;
  isActive: boolean;
};

export type UsePixabaySlotsResult = {
  slots: PixabaySlot[];
  onActiveEnded: () => void;
};

function nextSlotIndex(slot: number): number {
  return (slot + 1) % SLOT_COUNT;
}

function isAtEnd(video: HTMLVideoElement): boolean {
  return (
    Number.isFinite(video.duration) &&
    video.duration > 0 &&
    video.currentTime >= video.duration - NEAR_END_TOLERANCE_SEC
  );
}

function safePlay(video: HTMLVideoElement): void {
  void video.play().catch(() => {});
}

function rewind(video: HTMLVideoElement): void {
  video.currentTime = 0;
}

function emptySlotStrings(): string[] {
  return Array.from({ length: SLOT_COUNT }, () => '');
}

function emptySlotFlavors(): (VideoFlavor | null)[] {
  return Array.from({ length: SLOT_COUNT }, () => null);
}

type SlotSearch = {
  srcs: readonly string[];
  flavors: readonly (VideoFlavor | null)[];
  flavor: VideoFlavor;
  active: number;
  library: PixabayFlavorLibrary;
  readyOnly: boolean;
};

function findFlavorSlot(search: SlotSearch): number {
  return search.srcs.findIndex(
    (src, slot) =>
      slot !== search.active &&
      search.flavors[slot] === search.flavor &&
      src !== '' &&
      (!search.readyOnly || search.library.isReady(src)),
  );
}

function slotHasFlavor(
  slot: number,
  flavor: VideoFlavor,
  srcs: readonly string[],
  flavors: readonly (VideoFlavor | null)[],
): boolean {
  return srcs[slot] !== '' && flavors[slot] === flavor;
}

export function usePixabaySlots(flavor: VideoFlavor, isPlaying: boolean): UsePixabaySlotsResult {
  const firstVideoRef = useRef<HTMLVideoElement | null>(null);
  const secondVideoRef = useRef<HTMLVideoElement | null>(null);
  const thirdVideoRef = useRef<HTMLVideoElement | null>(null);
  const videoRefs = useMemo<RefObject<HTMLVideoElement | null>[]>(
    () => [firstVideoRef, secondVideoRef, thirdVideoRef],
    [],
  );

  const [activeIdx, setActiveIdx] = useState(0);
  const activeIdxRef = useRef(0);

  const [srcs, setSrcs] = useState<string[]>(emptySlotStrings);
  const srcsRef = useRef<string[]>(emptySlotStrings());
  const slotFlavorsRef = useRef<(VideoFlavor | null)[]>(emptySlotFlavors());

  const [library] = useState(() => new PixabayFlavorLibrary());

  const flavorRef = useLatestRef(flavor);
  const isPlayingRef = useLatestRef(isPlaying);

  const adapterReadyRef = useRef(false);
  const inflightFetchesRef = useRef(new Set<VideoFlavor>());
  const lastRefreshAtRef = useRef(new Map<VideoFlavor, number>());
  const pendingDownloadsRef = useRef<PixabayVideoDownloaded[]>([]);

  const writeSrcs = useCallback((next: string[]) => {
    srcsRef.current = next;
    setSrcs(next);
  }, []);

  const activateSlot = useCallback(
    (slot: number) => {
      activeIdxRef.current = slot;
      setActiveIdx(slot);
      const video = videoRefs[slot].current;
      if (video && isPlayingRef.current) {
        safePlay(video);
      }
    },
    [isPlayingRef, videoRefs],
  );

  const ensureAdapter = useCallback(async () => {
    if (adapterReadyRef.current) {
      return;
    }
    await playbackAdapter.init();
    adapterReadyRef.current = true;
  }, []);

  const toUrl = useCallback((path: string): string => {
    return playbackAdapter.toMediaUrl(path);
  }, []);

  const setSlotSrc = useCallback(
    (slot: number, url: string, slotFlavor: VideoFlavor) => {
      const next = [...srcsRef.current];
      next[slot] = url;
      writeSrcs(next);
      slotFlavorsRef.current[slot] = slotFlavor;
      library.markUnready(url);

      const video = videoRefs[slot].current;
      if (video) {
        video.addEventListener(
          'canplay',
          () => {
            library.markReady(url);
          },
          { once: true },
        );
      }
    },
    [library, videoRefs, writeSrcs],
  );

  const activateWhenReady = useCallback(
    (slot: number, url: string, slotFlavor: VideoFlavor) => {
      const tryActivate = () => {
        if (
          srcsRef.current[slot] === url &&
          slotFlavorsRef.current[slot] === slotFlavor &&
          flavorRef.current === slotFlavor
        ) {
          activateSlot(slot);
        }
      };

      if (library.isReady(url)) {
        tryActivate();
        return;
      }

      const video = videoRefs[slot].current;
      if (!video) {
        return;
      }

      video.addEventListener('canplay', tryActivate, { once: true });
    },
    [activateSlot, flavorRef, library, videoRefs],
  );

  const restartIfEnded = useCallback(
    (slot: number) => {
      const video = videoRefs[slot].current;
      if (!video) {
        return;
      }

      if (video.ended || isAtEnd(video)) {
        rewind(video);
      }

      if (isPlayingRef.current) {
        safePlay(video);
      }
    },
    [isPlayingRef, videoRefs],
  );

  const refillSiblingSlots = useCallback(
    (slotFlavor: VideoFlavor) => {
      const active = activeIdxRef.current;
      for (let slot = 0; slot < SLOT_COUNT; slot++) {
        if (slot === active) {
          continue;
        }
        if (slotFlavorsRef.current[slot] !== slotFlavor || !srcsRef.current[slot]) {
          const url = library.pullUrl(slotFlavor, false) ?? library.pullUrl(slotFlavor, true);
          if (typeof url === 'string' && url !== '') {
            setSlotSrc(slot, url, slotFlavor);
          }
        }
      }
    },
    [library, setSlotSrc],
  );

  const ensureFlavorPlayback = useCallback(
    (slotFlavor: VideoFlavor) => {
      const active = activeIdxRef.current;

      if (slotHasFlavor(active, slotFlavor, srcsRef.current, slotFlavorsRef.current)) {
        refillSiblingSlots(slotFlavor);
        return;
      }

      const search = {
        srcs: srcsRef.current,
        flavors: slotFlavorsRef.current,
        flavor: slotFlavor,
        active,
        library,
      };
      const readySlot = findFlavorSlot({ ...search, readyOnly: true });
      if (readySlot >= 0) {
        activateSlot(readySlot);
        refillSiblingSlots(slotFlavor);
        return;
      }

      const candidateSlot = findFlavorSlot({ ...search, readyOnly: false });
      if (candidateSlot >= 0) {
        activateWhenReady(candidateSlot, srcsRef.current[candidateSlot], slotFlavor);
        refillSiblingSlots(slotFlavor);
        return;
      }

      const playSlot = nextSlotIndex(active);
      const preloadSlot = nextSlotIndex(playSlot);

      const playUrl = library.pullUrl(slotFlavor, true);
      if (typeof playUrl !== 'string' || playUrl === '') {
        return;
      }
      setSlotSrc(playSlot, playUrl, slotFlavor);
      activateWhenReady(playSlot, playUrl, slotFlavor);

      const preloadUrl = library.pullUrl(slotFlavor, false) ?? library.pullUrl(slotFlavor, true);
      if (typeof preloadUrl === 'string' && preloadUrl !== '') {
        setSlotSrc(preloadSlot, preloadUrl, slotFlavor);
      }
    },
    [activateSlot, activateWhenReady, library, refillSiblingSlots, setSlotSrc],
  );

  const refreshFlavor = async (slotFlavor: VideoFlavor) => {
    const now = Date.now();
    const lastRefresh = lastRefreshAtRef.current.get(slotFlavor) ?? 0;
    if (now - lastRefresh < REFRESH_COOLDOWN_MS) {
      return;
    }
    if (inflightFetchesRef.current.has(slotFlavor)) {
      return;
    }

    lastRefreshAtRef.current.set(slotFlavor, now);
    inflightFetchesRef.current.add(slotFlavor);

    try {
      await ensureAdapter();
      const paths = await fetchPixabayVideos(slotFlavor);
      const urls = paths.map((path) => toUrl(path));
      library.registerUrls(slotFlavor, urls);

      if (flavorRef.current === slotFlavor) {
        ensureFlavorPlayback(slotFlavor);
      }
    } finally {
      inflightFetchesRef.current.delete(slotFlavor);
    }
  };
  const refreshFlavorRef = useLatestRef(refreshFlavor);

  const ingestDownload = useCallback(
    (event: PixabayVideoDownloaded) => {
      if (!adapterReadyRef.current) {
        pendingDownloadsRef.current.push(event);
        return;
      }

      if (!isVideoFlavor(event.flavor)) {
        return;
      }
      const downloadedFlavor = event.flavor;
      library.registerUrls(downloadedFlavor, [toUrl(event.path)]);

      if (typeof event.evictedPath === 'string' && event.evictedPath !== '') {
        library.removeUrl(downloadedFlavor, toUrl(event.evictedPath));
      }

      if (downloadedFlavor === flavorRef.current) {
        ensureFlavorPlayback(downloadedFlavor);
      }
    },
    [ensureFlavorPlayback, flavorRef, library, toUrl],
  );

  const flushPendingDownloads = useCallback(() => {
    if (!adapterReadyRef.current || pendingDownloadsRef.current.length === 0) {
      return;
    }

    const queued = [...pendingDownloadsRef.current];
    pendingDownloadsRef.current = [];
    queued.forEach((event) => ingestDownload(event));
  }, [ingestDownload]);

  const handleEnded = useCallback(() => {
    const currentFlavor = flavorRef.current;
    const active = activeIdxRef.current;
    const upcoming = nextSlotIndex(active);
    const upcomingSrc = srcsRef.current[upcoming];

    const refillActiveFromQueue = () => {
      const refill = library.pullUrl(currentFlavor, false) ?? library.pullUrl(currentFlavor, true);
      if (typeof refill === 'string' && refill !== '') {
        setSlotSrc(active, refill, currentFlavor);
      }
    };

    if (
      slotHasFlavor(upcoming, currentFlavor, srcsRef.current, slotFlavorsRef.current) &&
      library.isReady(upcomingSrc)
    ) {
      activateSlot(upcoming);
      refillActiveFromQueue();
    } else {
      const readySlot = findFlavorSlot({
        srcs: srcsRef.current,
        flavors: slotFlavorsRef.current,
        flavor: currentFlavor,
        active,
        library,
        readyOnly: true,
      });
      if (readySlot >= 0) {
        activateSlot(readySlot);
        refillActiveFromQueue();
      } else {
        const fallback = library.pullUrl(currentFlavor, true);
        if (typeof fallback === 'string' && fallback !== '') {
          setSlotSrc(upcoming, fallback, currentFlavor);
          activateWhenReady(upcoming, fallback, currentFlavor);
        }
        restartIfEnded(active);
      }
    }

    if (library.queueLength(currentFlavor) < MIN_QUEUE_BEFORE_REFRESH) {
      void refreshFlavorRef.current(currentFlavor);
    }
  }, [
    activateSlot,
    activateWhenReady,
    flavorRef,
    library,
    refreshFlavorRef,
    restartIfEnded,
    setSlotSrc,
  ]);

  useEffect(() => {
    ensureFlavorPlayback(flavor);
    void refreshFlavorRef.current(flavor);
    void refreshFlavorRef.current(getNextFlavor(flavor));
  }, [ensureFlavorPlayback, flavor, refreshFlavorRef]);

  useEffect(() => {
    const video = videoRefs[activeIdx].current;
    if (!video) {
      return;
    }

    if (isPlaying) {
      safePlay(video);
    } else {
      video.pause();
    }
  }, [activeIdx, isPlaying, videoRefs]);

  useEffect(() => {
    const video = videoRefs[activeIdx].current;
    if (!video) {
      return undefined;
    }

    let lastTime = video.currentTime;
    const onTimeUpdate = () => {
      lastTime = video.currentTime;
    };
    video.addEventListener('timeupdate', onTimeUpdate);

    const stallTimer = window.setInterval(() => {
      if (!video.paused && video.currentTime === lastTime) {
        handleEnded();
      }
    }, STALL_TIMEOUT_MS);

    return () => {
      video.removeEventListener('timeupdate', onTimeUpdate);
      window.clearInterval(stallTimer);
    };
  }, [activeIdx, handleEnded, videoRefs]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void ensureAdapter().then(() => {
      flushPendingDownloads();
      ensureFlavorPlayback(flavorRef.current);
      return undefined;
    });

    void onPixabayVideoDownloaded((event) => {
      ingestDownload(event);
    }).then((fn) => {
      if (disposed) {
        void Promise.resolve(fn()).catch(() => {});
        return undefined;
      }
      unlisten = fn;
      return undefined;
    });

    return () => {
      disposed = true;
      if (!unlisten) {
        return;
      }
      void Promise.resolve(unlisten()).catch(() => {});
    };
  }, [ensureAdapter, ensureFlavorPlayback, flavorRef, flushPendingDownloads, ingestDownload]);

  const slots: PixabaySlot[] = [
    { id: 'primary', ref: firstVideoRef, src: srcs[0], isActive: activeIdx === 0 },
    { id: 'secondary', ref: secondVideoRef, src: srcs[1], isActive: activeIdx === 1 },
    { id: 'secondary', ref: thirdVideoRef, src: srcs[2], isActive: activeIdx === 2 },
  ];

  return { slots, onActiveEnded: handleEnded };
}
