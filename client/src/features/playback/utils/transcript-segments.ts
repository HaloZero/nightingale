/**
 * Transcript post-processing for the playback lyrics UI.
 * Splits segments that exceed a word limit so lines wrap predictably on screen.
 */

import type { Segment } from '@/types/Transcript';

/** Seconds before the first lyric that "Skip intro" still applies (keyboard + HUD). */
export const INTRO_SKIP_LEAD_SEC = 3;

const DEFAULT_CHUNK_WORDS = 8;

/**
 * Splits segments with more than `maxWords` words into multiple segments,
 * preserving word-level timing from the original chunk.
 */
export function splitLongSegments(
  segments: Segment[],
  maxWords: number = DEFAULT_CHUNK_WORDS,
): Segment[] {
  const result: Segment[] = [];
  for (const seg of segments) {
    if (seg.words.length <= maxWords) {
      result.push(seg);
      continue;
    }
    for (let i = 0; i < seg.words.length; i += maxWords) {
      const chunk = seg.words.slice(i, i + maxWords);
      result.push({
        text: chunk.map((w) => w.word).join(' '),
        start: chunk[0].start,
        end: chunk[chunk.length - 1].end,
        words: chunk,
      });
    }
  }
  return result;
}
