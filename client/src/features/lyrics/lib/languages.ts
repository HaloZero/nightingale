export const LANGUAGES = [
  ['en', 'English'],
  ['es', 'Spanish'],
  ['fr', 'French'],
  ['de', 'German'],
  ['it', 'Italian'],
  ['pt', 'Portuguese'],
  ['ru', 'Russian'],
  ['ja', 'Japanese'],
  ['ko', 'Korean'],
  ['zh', 'Mandarin'],
  ['yue', 'Cantonese'],
  ['ar', 'Arabic'],
  ['hi', 'Hindi'],
  ['nl', 'Dutch'],
  ['pl', 'Polish'],
  ['sv', 'Swedish'],
  ['tr', 'Turkish'],
  ['uk', 'Ukrainian'],
  ['cs', 'Czech'],
  ['ro', 'Romanian'],
  ['hu', 'Hungarian'],
] as const;

const LANGUAGE_NAMES = new Map<string, string>(LANGUAGES);

export function getLanguageName(language: string): string {
  return LANGUAGE_NAMES.get(language.toLowerCase()) ?? language.toUpperCase();
}
