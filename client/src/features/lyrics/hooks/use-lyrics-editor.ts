import { useMemo, useState } from 'react';

import { useSaveLyricsMutation } from '@/features/lyrics/mutations/use-save-lyrics-mutation';
import { useInitialLyrics } from '@/features/lyrics/queries/use-lyrics';
import { normalizeLines } from '@/features/lyrics/utils/edit-lyrics';
import type { Song } from '@/types/Song';

export type UseLyricsEditorArgs = {
  song: Song | null;
  onSaved: () => void;
};

export type LyricsEditorState = {
  text: string;
  setText: (text: string) => void;
  loadingInitial: boolean;
  saving: boolean;
  normalized: string[];
  isDirty: boolean;
  canSave: boolean;
  handleSave: () => Promise<void>;
};

export function useLyricsEditor({ song, onSaved }: UseLyricsEditorArgs): LyricsEditorState {
  const fileHash = song?.file_hash ?? null;

  const lyricsQuery = useInitialLyrics(fileHash);

  // `override` is the user-edited buffer; null means "show the loaded value".
  // Resetting it on song change follows the React-recommended "store previous
  // prop, reset during render" pattern, which avoids the extra-render flicker
  // of a useEffect.
  const [override, setOverride] = useState<string | null>(null);
  const [lastHash, setLastHash] = useState<string | null>(fileHash);

  if (lastHash !== fileHash) {
    setLastHash(fileHash);
    setOverride(null);
  }

  const loadedText = lyricsQuery.data ?? '';
  const text = override ?? loadedText;
  const isDirty = override !== null && override !== loadedText;

  const normalized = useMemo(() => normalizeLines(text), [text]);

  const saveMutation = useSaveLyricsMutation();

  const loadingInitial = lyricsQuery.isLoading;
  const saving = saveMutation.isLoading;
  const canSave = !saving && !loadingInitial && normalized.length > 0 && isDirty;

  const handleSave = async () => {
    if (!canSave || !song) {
      return;
    }
    saveMutation.mutate(
      { hash: song.file_hash, lines: normalized, title: song.title },
      { onSuccess: onSaved },
    );
  };

  return {
    text,
    setText: setOverride,
    loadingInitial,
    saving,
    normalized,
    isDirty,
    canSave,
    handleSave,
  };
}
