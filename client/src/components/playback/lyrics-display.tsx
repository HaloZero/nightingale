import { usePlaybackTransportActions, usePlaybackTransportState } from "@/contexts/playback";
import { cn } from "@/lib/utils";
import type { AppConfig } from "@/types/AppConfig";
import type { Segment, Word } from "@/types/Transcript";
import { memo, useEffect, useRef, useState } from "react";

// Timing offsets: lyrics/words appear slightly before their actual start
// so the visual transition feels in sync with the audio.
const LYRICS_LEAD = 0.15;
const WORD_HIGHLIGHT_LEAD = 0.25;

const COUNTDOWN_DURATION = 3.0;
const COUNTDOWN_GAP_THRESHOLD = 3.5;

// Grace period after a segment ends before it disappears
const SEGMENT_LINGER = 0.5;

interface WordStyle {
  rgb: string;
  opacity: number;
}

type LyricsColors = Record<
  "unsung" | "unsungEstimated" | "sung" | "nextLine" | "nextLineEstimated",
  WordStyle
>;

const STYLES: LyricsColors = {
  unsung: { rgb: "rgb(255,255,255)", opacity: 0.5 },
  unsungEstimated: { rgb: "rgb(255,200,100)", opacity: 0.4 },
  sung: { rgb: "rgb(255,255,255)", opacity: 1.0 },
  nextLine: { rgb: "rgb(255,255,255)", opacity: 0.35 },
  nextLineEstimated: { rgb: "rgb(255,200,100)", opacity: 0.25 },
};

const unsungStyle = (word: Word, colors: LyricsColors): WordStyle =>
  word.estimated ? colors.unsungEstimated : colors.unsung;

const nextLineStyle = (word: Word, colors: LyricsColors): WordStyle =>
  word.estimated ? colors.nextLineEstimated : colors.nextLine;

function lerp(a: number, b: number, t: number) {
  return a + (b - a) * t;
}

function interpolateStyle(from: WordStyle, to: WordStyle, t: number): WordStyle {
  const p = Math.max(0, Math.min(1, t));
  if (from.rgb === to.rgb) {
    return { rgb: to.rgb, opacity: lerp(from.opacity, to.opacity, p) };
  }
  const fm = from.rgb.match(/\d+/g)!;
  const tm = to.rgb.match(/\d+/g)!;
  const r = Math.round(lerp(+fm[0], +tm[0], p));
  const g = Math.round(lerp(+fm[1], +tm[1], p));
  const b = Math.round(lerp(+fm[2], +tm[2], p));
  return {
    rgb: `rgb(${r},${g},${b})`,
    opacity: lerp(from.opacity, to.opacity, p),
  };
}

// --- Segment search ---

/**
 * Finds the segment index that should be displayed at a given `time`.
 * Uses `hint` (the last known index) to skip already-passed segments.
 * Prefers the *next* segment when the current time falls in the lead-in window.
 */
function findCurrentSegment(segments: Segment[], time: number, hint: number): number {
  const start = hint < segments.length && time >= segments[hint].start - LYRICS_LEAD ? hint : 0;

  for (let i = start; i < segments.length; i++) {
    if (time >= segments[i].end + SEGMENT_LINGER) {
      const next = i + 1;

      // Through a short pause, keep the finished line current until the next
      // line's lead-in begins, so the switch happens when the new line starts
      // rather than when the old one ends.
      if (
        next < segments.length &&
        segments[next].start - segments[i].end < COUNTDOWN_GAP_THRESHOLD &&
        time < segments[next].start - LYRICS_LEAD
      ) {
        return i;
      }

      continue;
    }

    // If we're already in the lead-in of the next segment, jump ahead
    const next = i + 1;
    if (next < segments.length && time >= segments[next].start - LYRICS_LEAD) {
      return next;
    }

    return i;
  }

  return Math.max(0, segments.length - 1);
}

// --- Per-frame DOM updates (called via rAF subscriber, no React re-renders) ---

function computeWordStyle(
  word: Word,
  time: number,
  isActive: boolean,
  colors: LyricsColors,
): WordStyle {
  const base = unsungStyle(word, colors);
  if (!isActive) return base;

  const wStart = word.start - WORD_HIGHLIGHT_LEAD;
  const wEnd = word.end - WORD_HIGHLIGHT_LEAD;

  if (time >= wEnd) return colors.sung;
  if (time >= wStart) {
    return interpolateStyle(base, colors.sung, (time - wStart) / (wEnd - wStart));
  }
  return base;
}

function updateWordSpans(
  spans: (HTMLSpanElement | null)[],
  words: Word[],
  time: number,
  isActive: boolean,
  colors: LyricsColors,
) {
  for (let i = 0; i < words.length; i++) {
    const span = spans[i];
    if (!span) continue;
    const s = computeWordStyle(words[i], time, isActive, colors);
    span.style.color = s.rgb;
    span.style.opacity = String(s.opacity);
  }
}

function updateCountdown(el: HTMLSpanElement | null, showCountdown: boolean, timeUntil: number) {
  if (!el) {
    return;
  }

  if (showCountdown) {
    el.style.display = "";
    el.textContent = String(Math.ceil(timeUntil));
  } else {
    el.style.display = "none";
  }
}

// --- Word rendering ---

interface WordTokenProps {
  word: Word;
  hasReading: boolean;
  isLast: boolean;
  readingClass: string;
  refSetter?: (el: HTMLSpanElement | null) => void;
  style: WordStyle;
}

function WordToken({ word, hasReading, isLast, readingClass, refSetter, style }: WordTokenProps) {
  return (
    <span
      ref={refSetter}
      className={hasReading ? "inline-flex flex-col items-center leading-tight" : undefined}
      style={{ color: style.rgb, opacity: style.opacity }}
    >
      {hasReading && (
        <span className={`block leading-tight font-medium opacity-80 ${readingClass}`}>
          {word.reading ?? "\u00A0"}
        </span>
      )}
      <span>{word.word}</span>
      {!hasReading && !isLast ? " " : ""}
    </span>
  );
}

type LyricsVerticalPosition = NonNullable<AppConfig["lyrics_vertical_position"]>;

type LyricsHorizontalPosition = NonNullable<AppConfig["lyrics_horizontal_position"]>;

const verticalClass: Record<LyricsVerticalPosition, string> = {
  bottom: "top-[8rem] bottom-[calc(2rem+env(safe-area-inset-bottom))] justify-end sm:bottom-[60px]",
  center: "inset-y-[6rem] justify-center",
  top: "top-[calc(2rem+env(safe-area-inset-top))] bottom-[8rem] justify-start overflow-visible sm:top-[60px]",
};

const horizontalItemsClass: Record<LyricsHorizontalPosition, string> = {
  left: "items-start",
  center: "items-center",
  right: "items-end",
};

const horizontalTextClass: Record<LyricsHorizontalPosition, string> = {
  left: "text-left justify-start",
  center: "text-center justify-center",
  right: "text-right justify-end",
};

const COUNTDOWN_CLASS =
  "absolute -top-12 left-2 z-10 flex size-10 items-center justify-center rounded-full bg-black/40 text-[1rem] font-bold text-white sm:-left-9 sm:-top-9";

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

/** Style-override hook for alternate render targets (e.g. the Chromecast
 * receiver) -- every field defaults to today's exact desktop appearance, so
 * the desktop call site needs no changes. */
interface LyricsDisplayClassNames {
  container?: string;
  currentPill?: string;
  currentLine?: string;
  nextPill?: string;
  nextLine?: string;
}

interface LyricsDisplayProps {
  segments: Segment[];
  verticalPosition?: LyricsVerticalPosition | null;
  horizontalPosition?: LyricsHorizontalPosition | null;
  colors?: LyricsColors;
  classNames?: LyricsDisplayClassNames;
}

function LyricsDisplayImpl({
  segments,
  verticalPosition = "bottom",
  horizontalPosition = "center",
  colors = STYLES,
  classNames,
}: LyricsDisplayProps) {
  const { isPlaying, paused } = usePlaybackTransportState();
  const { subscribe, getCurrentTime } = usePlaybackTransportActions();
  const animate = isPlaying && !paused;

  const [segIdx, setSegIdx] = useState(() =>
    segments.length === 0 ? 0 : findCurrentSegment(segments, getCurrentTime(), 0),
  );

  const hintRef = useRef(0);
  const wordRefs = useRef<(HTMLSpanElement | null)[]>([]);
  const countdownRef = useRef<HTMLSpanElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const nextContainerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (segments.length === 0) return;

    let raf = 0;
    let cancelled = false;

    const apply = (time: number) => {
      const idx = findCurrentSegment(segments, time, hintRef.current);
      if (idx !== hintRef.current) {
        hintRef.current = idx;
        setSegIdx(idx);
      }

      const seg = segments[idx];
      const isActive = time >= seg.start - LYRICS_LEAD && time <= seg.end + SEGMENT_LINGER;

      const gapBefore = idx === 0 ? seg.start : seg.start - segments[idx - 1].end;
      const timeUntil = seg.start - time;
      const showCountdown =
        gapBefore >= COUNTDOWN_GAP_THRESHOLD && timeUntil > 0 && timeUntil <= COUNTDOWN_DURATION;

      // After a line ends, keep it on screen through a short pause until the
      // next line starts (findCurrentSegment holds idx on the finished line).
      const nextStart = idx + 1 < segments.length ? segments[idx + 1].start : Infinity;
      const bridgeShortGap =
        time > seg.end + SEGMENT_LINGER && nextStart - seg.end < COUNTDOWN_GAP_THRESHOLD;

      const showCurrent = isActive || showCountdown || bridgeShortGap;
      const hasNext = idx + 1 < segments.length;

      if (containerRef.current) containerRef.current.style.display = showCurrent ? "" : "none";
      if (nextContainerRef.current)
        nextContainerRef.current.style.display = showCurrent && hasNext ? "" : "none";

      updateCountdown(countdownRef.current, showCountdown, timeUntil);
      // Bridged finished lines are past every word's end, so treating them as
      // active keeps the already-sung colors instead of dropping to unsung.
      updateWordSpans(wordRefs.current, seg.words, time, isActive || bridgeShortGap, colors);
    };

    if (animate) {
      const loop = () => {
        if (cancelled) return;
        apply(getCurrentTime());
        raf = requestAnimationFrame(loop);
      };
      raf = requestAnimationFrame(loop);
      return () => {
        cancelled = true;
        cancelAnimationFrame(raf);
      };
    }

    apply(getCurrentTime());
    return subscribe((time) => apply(time));
  }, [segments, subscribe, getCurrentTime, animate, colors]);

  if (segments.length === 0) {
    return null;
  }

  const seg = segments[segIdx];
  const nextSeg = segIdx + 1 < segments.length ? segments[segIdx + 1] : null;

  wordRefs.current = [];

  const segHasReading = seg.words.some((w) => w.reading);
  const nextHasReading = nextSeg?.words.some((w) => w.reading) ?? false;

  const vertical = verticalPosition ?? "bottom";
  const horizontal = horizontalPosition ?? "center";

  return (
    <div
      className={cn(
        "pointer-events-none absolute inset-x-0 z-10 flex flex-col gap-2 overflow-hidden px-3 sm:px-10",
        verticalClass[vertical],
        horizontalItemsClass[horizontal],
        classNames?.container,
      )}
    >
      <div
        ref={containerRef}
        className={
          classNames?.currentPill ??
          "relative max-w-full rounded-lg bg-black/40 px-3 py-2 sm:px-5 sm:py-2.5"
        }
        style={{ display: "none" }}
      >
        <span ref={countdownRef} className={COUNTDOWN_CLASS} style={{ display: "none" }} />
        {seg.words.length > 0 && (
          <p
            className={
              classNames?.currentLine ??
              lineClass(
                segHasReading,
                "text-[clamp(1.35rem,7svh,2.5rem)] leading-tight font-bold",
                "gap-x-3 gap-y-1",
                horizontal,
              )
            }
          >
            {seg.words.map((word, wi) => (
              <WordToken
                key={`${segIdx}-${wi}`}
                word={word}
                hasReading={segHasReading}
                isLast={wi === seg.words.length - 1}
                readingClass="text-[clamp(0.65rem,3svh,1rem)]"
                refSetter={(el) => {
                  wordRefs.current[wi] = el;
                }}
                style={colors.unsung}
              />
            ))}
          </p>
        )}
      </div>

      {nextSeg && (
        <div
          ref={nextContainerRef}
          className={
            classNames?.nextPill ?? "max-w-full rounded-md bg-black/25 px-3 py-1.5 sm:px-4"
          }
          style={{ display: "none" }}
        >
          <p
            className={
              classNames?.nextLine ??
              lineClass(
                nextHasReading,
                "text-[clamp(0.9rem,4.5svh,1.5rem)] leading-tight",
                "gap-x-2 gap-y-0.5",
                horizontal,
              )
            }
          >
            {nextSeg.words.map((word, wi) => (
              <WordToken
                key={wi}
                word={word}
                hasReading={nextHasReading}
                isLast={wi === nextSeg.words.length - 1}
                readingClass="text-[clamp(0.55rem,2.25svh,0.7rem)]"
                style={nextLineStyle(word, colors)}
              />
            ))}
          </p>
        </div>
      )}
    </div>
  );
}

export const LyricsDisplay = memo(LyricsDisplayImpl);
