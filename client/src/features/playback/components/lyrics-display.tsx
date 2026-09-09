import { memo, useLayoutEffect, useRef, useState, type RefObject } from 'react';

import { clampPlaybackScale } from '@/features/playback/lib/display-scale';
import {
  usePlaybackTransportActions,
  usePlaybackTransportState,
} from '@/features/playback/providers';
import {
  BUBBLE_COUNTDOWN_SEC,
  findCurrentSegment,
  GAP_THRESHOLD_SEC,
  LYRICS_LEAD,
  SEGMENT_LINGER,
  WORD_HIGHLIGHT_LEAD,
} from '@/features/playback/utils/lyrics-gap';
import { cn } from '@/shared/utils/cn';
import type { AppConfig } from '@/types/AppConfig';
import type { Segment, Word } from '@/types/Transcript';

type WordStyle = {
  rgb: string;
  opacity: number;
};

const STYLES = {
  unsung: { rgb: 'rgb(255,255,255)', opacity: 0.5 },
  unsungEstimated: { rgb: 'rgb(255,200,100)', opacity: 0.4 },
  sung: { rgb: 'rgb(255,255,255)', opacity: 1.0 },
  nextLine: { rgb: 'rgb(255,255,255)', opacity: 0.35 },
  nextLineEstimated: { rgb: 'rgb(255,200,100)', opacity: 0.25 },
} as const;

const unsungStyle = (word: Word): WordStyle =>
  word.estimated === true ? STYLES.unsungEstimated : STYLES.unsung;

const nextLineStyle = (word: Word): WordStyle =>
  word.estimated === true ? STYLES.nextLineEstimated : STYLES.nextLine;

function lerp(a: number, b: number, t: number) {
  return a + (b - a) * t;
}

function interpolateStyle(from: WordStyle, to: WordStyle, t: number): WordStyle {
  const p = Math.max(0, Math.min(1, t));
  if (from.rgb === to.rgb) {
    return { rgb: to.rgb, opacity: lerp(from.opacity, to.opacity, p) };
  }
  const fm = from.rgb.match(/\d+/g);
  const tm = to.rgb.match(/\d+/g);
  if (fm === null || tm === null) {
    return to;
  }
  const r = Math.round(lerp(+fm[0], +tm[0], p));
  const g = Math.round(lerp(+fm[1], +tm[1], p));
  const b = Math.round(lerp(+fm[2], +tm[2], p));
  return {
    rgb: `rgb(${r},${g},${b})`,
    opacity: lerp(from.opacity, to.opacity, p),
  };
}

// --- Per-frame DOM updates (called via rAF subscriber, no React re-renders) ---

function computeWordStyle(word: Word, time: number, isActive: boolean): WordStyle {
  const base = unsungStyle(word);
  if (!isActive) {
    return base;
  }

  const wStart = word.start - WORD_HIGHLIGHT_LEAD;
  const wEnd = word.end - WORD_HIGHLIGHT_LEAD;

  if (time >= wEnd) {
    return STYLES.sung;
  }
  if (time >= wStart) {
    return interpolateStyle(base, STYLES.sung, (time - wStart) / (wEnd - wStart));
  }
  return base;
}

function updateWordSpans(
  spans: (HTMLSpanElement | null)[],
  words: Word[],
  time: number,
  isActive: boolean,
) {
  for (let i = 0; i < words.length; i++) {
    const span = spans[i];
    if (!span) {
      continue;
    }
    const s = computeWordStyle(words[i], time, isActive);
    span.style.color = s.rgb;
    span.style.opacity = String(s.opacity);
  }
}

function updateCountdown(el: HTMLSpanElement | null, showCountdown: boolean, timeUntil: number) {
  if (!el) {
    return;
  }

  if (showCountdown) {
    el.style.display = '';
    el.textContent = String(Math.ceil(timeUntil));
  } else {
    el.style.display = 'none';
  }
}

// --- Word rendering ---

type WordTokenProps = {
  word: Word;
  hasReading: boolean;
  isLast: boolean;
  readingClass: string;
  refSetter?: (el: HTMLSpanElement | null) => void;
  style: WordStyle;
};

function WordToken({ word, hasReading, isLast, readingClass, refSetter, style }: WordTokenProps) {
  return (
    <span
      ref={refSetter}
      className={hasReading ? 'inline-flex flex-col items-center leading-tight' : undefined}
      style={{ color: style.rgb, opacity: style.opacity }}
    >
      {hasReading && (
        <span className={`block leading-tight font-medium opacity-80 ${readingClass}`}>
          {word.reading ?? '\u00A0'}
        </span>
      )}
      <span>{word.word}</span>
      {!hasReading && !isLast ? ' ' : ''}
    </span>
  );
}

type LyricsVerticalPosition = NonNullable<AppConfig['lyrics_vertical_position']>;

type LyricsHorizontalPosition = NonNullable<AppConfig['lyrics_horizontal_position']>;

const verticalClass: Record<LyricsVerticalPosition, string> = {
  bottom: 'top-[8rem] bottom-[calc(2rem+env(safe-area-inset-bottom))] justify-end sm:bottom-[60px]',
  center: 'inset-y-[6rem] justify-center',
  top: 'top-[calc(2rem+env(safe-area-inset-top))] bottom-[8rem] justify-start overflow-visible sm:top-[60px]',
};

const horizontalItemsClass: Record<LyricsHorizontalPosition, string> = {
  left: 'items-start',
  center: 'items-center',
  right: 'items-end',
};

const horizontalTextClass: Record<LyricsHorizontalPosition, string> = {
  left: 'text-left justify-start',
  center: 'text-center justify-center',
  right: 'text-right justify-end',
};

const COUNTDOWN_CLASS =
  'absolute -top-12 left-2 z-10 flex size-10 items-center justify-center rounded-full bg-black/40 text-[1rem] font-bold text-white sm:-left-9 sm:-top-9';

const lineClass = (
  hasReading: boolean,
  base: string,
  gap: string,
  horizontalPosition: LyricsHorizontalPosition,
) =>
  hasReading
    ? `flex flex-wrap items-end ${horizontalTextClass[horizontalPosition]} ${gap} ${base}`
    : `${horizontalTextClass[horizontalPosition]} ${base}`;

// --- Component ---

const hasReading = (word: Word): boolean => typeof word.reading === 'string' && word.reading !== '';

const wordKey = (word: Word): string => `${word.start}-${word.end}-${word.word}`;

type SegmentVisibility = {
  segment: Segment;
  isActive: boolean;
  bridgeShortGap: boolean;
  showCountdown: boolean;
  showCurrent: boolean;
  showNext: boolean;
  timeUntil: number;
};

const segmentEdges = (segments: readonly Segment[], index: number) => ({
  gapBefore: index === 0 ? segments[index].start : segments[index].start - segments[index - 1].end,
  nextStart: index + 1 < segments.length ? segments[index + 1].start : Infinity,
});

function segmentVisibility(
  segments: readonly Segment[],
  index: number,
  time: number,
): SegmentVisibility {
  const segment = segments[index];
  const { gapBefore, nextStart } = segmentEdges(segments, index);
  const timeUntil = segment.start - time;
  const isActive = time >= segment.start - LYRICS_LEAD && time <= segment.end + SEGMENT_LINGER;
  const showCountdown =
    gapBefore >= GAP_THRESHOLD_SEC && timeUntil > 0 && timeUntil <= BUBBLE_COUNTDOWN_SEC;
  const bridgeShortGap =
    time > segment.end + SEGMENT_LINGER && nextStart - segment.end < GAP_THRESHOLD_SEC;
  const showCurrent = isActive || showCountdown || bridgeShortGap;
  const nextIsContinuous =
    index + 1 < segments.length && nextStart - segment.end < GAP_THRESHOLD_SEC;
  const inLongGap = gapBefore >= GAP_THRESHOLD_SEC && timeUntil > LYRICS_LEAD;
  const showNext = [showCurrent, nextIsContinuous, !inLongGap || showCountdown].every(Boolean);

  return {
    segment,
    isActive,
    bridgeShortGap,
    showCountdown,
    showCurrent,
    showNext,
    timeUntil,
  };
}

/** Style-override hook for alternate render targets (e.g. the Chromecast
 * receiver) -- every field defaults to today's exact desktop appearance, so
 * the desktop call site needs no changes. */
type LyricsDisplayClassNames = {
  container?: string;
  currentPill?: string;
  currentLine?: string;
  nextPill?: string;
  nextLine?: string;
};

type LyricsDisplayProps = {
  segments: Segment[];
  verticalPosition?: LyricsVerticalPosition | null;
  horizontalPosition?: LyricsHorizontalPosition | null;
  scale?: number | null;
  classNames?: LyricsDisplayClassNames;
};

const lyricPositions = (props: LyricsDisplayProps) => ({
  vertical: props.verticalPosition ?? 'bottom',
  horizontal: props.horizontalPosition ?? 'center',
});

type ApplyVisibilityRefs = {
  hintRef: RefObject<number>;
  renderedIdxRef: RefObject<number>;
  appliedIdxRef: RefObject<number | null>;
  wordRefs: RefObject<(HTMLSpanElement | null)[]>;
  countdownRef: RefObject<HTMLSpanElement | null>;
  containerRef: RefObject<HTMLDivElement | null>;
  nextContainerRef: RefObject<HTMLDivElement | null>;
};

// Pushes one instant's worth of segment visibility/highlight state straight
// onto the DOM via refs, bypassing React's render for the per-frame
// (animate) or per-timeupdate (subscribe) cadence in the effect below --
// factored out so `LyricsDisplayImpl`'s own complexity stays readable.
function createApplyVisibility(
  segments: readonly Segment[],
  refs: ApplyVisibilityRefs,
  setSegIdx: (idx: number) => void,
): (time: number) => void {
  return (time: number) => {
    const idx = findCurrentSegment(segments, time, refs.hintRef.current);
    if (idx !== refs.hintRef.current) {
      refs.hintRef.current = idx;
      setSegIdx(idx);
    }

    // Wait for React to render the new segment before mutating its visibility.
    if (idx !== refs.renderedIdxRef.current) {
      return;
    }

    const visibility = segmentVisibility(segments, idx, time);

    if (refs.containerRef.current) {
      refs.containerRef.current.style.display = visibility.showCurrent ? '' : 'none';
    }
    if (refs.nextContainerRef.current) {
      refs.nextContainerRef.current.style.display = visibility.showNext ? '' : 'none';
    }

    updateCountdown(refs.countdownRef.current, visibility.showCountdown, visibility.timeUntil);
    // Bridged finished lines are past every word's end, so treating them as
    // active keeps the already-sung colors instead of dropping to unsung.
    updateWordSpans(
      refs.wordRefs.current,
      visibility.segment.words,
      time,
      visibility.isActive || visibility.bridgeShortGap,
    );
    refs.appliedIdxRef.current = idx;
  };
}

const NOOP = () => {};

type SyncVisibilityOptions = {
  segments: readonly Segment[];
  animate: boolean;
  subscribe: (fn: (time: number) => void) => () => void;
  getCurrentTime: () => number;
  setSegIdx: (idx: number) => void;
  refs: ApplyVisibilityRefs & { applyRef: RefObject<((time: number) => void) | null> };
};

// Drives the per-frame (animate) or per-timeupdate (subscribe) visibility
// sync effect: builds `apply`, runs it once immediately, then keeps it
// running until the returned cleanup fires. Factored out (alongside
// `createApplyVisibility`) so `LyricsDisplayImpl` itself only has to call
// it and stays under the complexity limit.
function syncSegmentVisibility(options: SyncVisibilityOptions): (() => void) | undefined {
  const { segments, animate, subscribe, getCurrentTime, setSegIdx, refs } = options;
  if (segments.length === 0) {
    return undefined;
  }

  let raf = 0;
  let cancelled = false;

  const apply = createApplyVisibility(segments, refs, setSegIdx);
  refs.applyRef.current = apply;

  const cleanup = () => {
    cancelled = true;
    cancelAnimationFrame(raf);
    if (refs.applyRef.current === apply) {
      refs.applyRef.current = null;
      refs.appliedIdxRef.current = null;
    }
  };

  apply(getCurrentTime());

  let unsubscribe = NOOP;
  if (animate) {
    const loop = () => {
      if (cancelled) {
        return;
      }
      apply(getCurrentTime());
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
  } else {
    unsubscribe = subscribe((time) => apply(time));
  }

  return () => {
    cleanup();
    unsubscribe();
  };
}

function lineFontSizeStyle(
  override: string | undefined,
  fontSize: string,
): { fontSize: string } | undefined {
  return override === undefined ? { fontSize } : undefined;
}

type CurrentLinePillProps = {
  containerRef: RefObject<HTMLDivElement | null>;
  countdownRef: RefObject<HTMLSpanElement | null>;
  segment: Segment;
  hasReading: boolean;
  horizontal: LyricsHorizontalPosition;
  fontSize: string;
  classNames: LyricsDisplayClassNames | undefined;
  wordRefs: RefObject<(HTMLSpanElement | null)[]>;
};

function CurrentLinePill({
  containerRef,
  countdownRef,
  segment,
  hasReading: segmentHasReading,
  horizontal,
  fontSize,
  classNames,
  wordRefs,
}: CurrentLinePillProps) {
  return (
    <div
      ref={containerRef}
      className={
        classNames?.currentPill ??
        'relative max-w-full rounded-lg bg-black/40 px-3 py-2 sm:px-5 sm:py-2.5'
      }
      style={{ display: 'none' }}
    >
      <span ref={countdownRef} className={COUNTDOWN_CLASS} style={{ display: 'none' }} />
      {segment.words.length > 0 && (
        <p
          className={
            classNames?.currentLine ??
            lineClass(segmentHasReading, 'leading-tight font-bold', 'gap-x-3 gap-y-1', horizontal)
          }
          style={lineFontSizeStyle(classNames?.currentLine, fontSize)}
        >
          {segment.words.map((word, wi) => (
            <WordToken
              key={wordKey(word)}
              word={word}
              hasReading={segmentHasReading}
              isLast={wi === segment.words.length - 1}
              readingClass="text-[0.4em]"
              refSetter={(el) => {
                wordRefs.current[wi] = el;
              }}
              style={STYLES.unsung}
            />
          ))}
        </p>
      )}
    </div>
  );
}

type NextLinePillProps = {
  containerRef: RefObject<HTMLDivElement | null>;
  segment: Segment;
  hasReading: boolean;
  horizontal: LyricsHorizontalPosition;
  fontSize: string;
  classNames: LyricsDisplayClassNames | undefined;
};

function NextLinePill({
  containerRef,
  segment,
  hasReading: segmentHasReading,
  horizontal,
  fontSize,
  classNames,
}: NextLinePillProps) {
  return (
    <div
      ref={containerRef}
      className={classNames?.nextPill ?? 'max-w-full rounded-md bg-black/25 px-3 py-1.5 sm:px-4'}
      style={{ display: 'none' }}
    >
      <p
        className={
          classNames?.nextLine ??
          lineClass(segmentHasReading, 'leading-tight', 'gap-x-2 gap-y-0.5', horizontal)
        }
        style={lineFontSizeStyle(classNames?.nextLine, fontSize)}
      >
        {segment.words.map((word, wi) => (
          <WordToken
            key={wordKey(word)}
            word={word}
            hasReading={segmentHasReading}
            isLast={wi === segment.words.length - 1}
            readingClass="text-[0.467em]"
            style={nextLineStyle(word)}
          />
        ))}
      </p>
    </div>
  );
}

function LyricsDisplayImpl(props: LyricsDisplayProps) {
  const { segments, classNames } = props;
  const { vertical, horizontal } = lyricPositions(props);
  const scale = clampPlaybackScale(props.scale);
  const currentFontSize = `clamp(${1.35 * scale}rem, ${7 * scale}svh, ${2.5 * scale}rem)`;
  const nextFontSize = `clamp(${0.9 * scale}rem, ${4.5 * scale}svh, ${1.5 * scale}rem)`;
  const { isPlaying, paused } = usePlaybackTransportState();
  const { subscribe, getCurrentTime } = usePlaybackTransportActions();
  const animate = isPlaying && !paused;

  const [segIdx, setSegIdx] = useState(() =>
    segments.length === 0 ? 0 : findCurrentSegment(segments, getCurrentTime(), 0),
  );

  const hintRef = useRef(0);
  const renderedIdxRef = useRef(segIdx);
  const appliedIdxRef = useRef<number | null>(null);
  const applyRef = useRef<((time: number) => void) | null>(null);
  const wordRefs = useRef<(HTMLSpanElement | null)[]>([]);
  const countdownRef = useRef<HTMLSpanElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const nextContainerRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(
    () =>
      syncSegmentVisibility({
        segments,
        animate,
        subscribe,
        getCurrentTime,
        setSegIdx,
        refs: {
          hintRef,
          renderedIdxRef,
          appliedIdxRef,
          wordRefs,
          countdownRef,
          containerRef,
          nextContainerRef,
          applyRef,
        },
      }),
    [segments, subscribe, getCurrentTime, animate],
  );

  // Synchronize visibility with the committed segment before paint.
  useLayoutEffect(() => {
    renderedIdxRef.current = segIdx;
    if (appliedIdxRef.current !== segIdx) {
      applyRef.current?.(getCurrentTime());
    }
  }, [segIdx, getCurrentTime]);

  if (segments.length === 0) {
    return null;
  }

  const seg = segments[segIdx];
  const nextSeg = segIdx + 1 < segments.length ? segments[segIdx + 1] : null;

  const segHasReading = seg.words.some(hasReading);
  const nextHasReading = nextSeg?.words.some(hasReading) ?? false;

  return (
    <div
      className={cn(
        'pointer-events-none absolute inset-x-0 z-10 flex flex-col gap-2 overflow-hidden px-3 sm:px-10',
        verticalClass[vertical],
        horizontalItemsClass[horizontal],
        classNames?.container,
      )}
    >
      <CurrentLinePill
        containerRef={containerRef}
        countdownRef={countdownRef}
        segment={seg}
        hasReading={segHasReading}
        horizontal={horizontal}
        fontSize={currentFontSize}
        classNames={classNames}
        wordRefs={wordRefs}
      />

      {nextSeg && (
        <NextLinePill
          containerRef={nextContainerRef}
          segment={nextSeg}
          hasReading={nextHasReading}
          horizontal={horizontal}
          fontSize={nextFontSize}
          classNames={classNames}
        />
      )}
    </div>
  );
}

export const LyricsDisplay = memo(LyricsDisplayImpl);
