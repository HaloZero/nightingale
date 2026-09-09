import type { KeyShift } from '@/types/KeyShift';

export const NOTE_NAMES = [
  'C',
  'C#',
  'D',
  'D#',
  'E',
  'F',
  'F#',
  'G',
  'G#',
  'A',
  'A#',
  'B',
] as const;

type NoteName = (typeof NOTE_NAMES)[number];

function parseKey(key: string): { index: number; quality: string } | null {
  const m = key.match(/^([A-G]#?)(m?)$/);

  if (!m) {
    return null;
  }

  const index = NOTE_NAMES.findIndex((note: NoteName) => note === m[1]);

  if (index < 0) {
    return null;
  }

  return { index, quality: m[2] };
}

export function calculateKeyShift(originalKey: string, keyOffset: number): KeyShift {
  const parsed = parseKey(originalKey);
  const pitchRatio = Math.pow(2, keyOffset / NOTE_NAMES.length);

  if (!parsed) {
    return { key: originalKey, keyOffset, pitchRatio };
  }

  const newIndex =
    (((parsed.index + keyOffset) % NOTE_NAMES.length) + NOTE_NAMES.length) % NOTE_NAMES.length;

  return {
    key: `${NOTE_NAMES[newIndex]}${parsed.quality}`,
    keyOffset,
    pitchRatio,
  };
}
