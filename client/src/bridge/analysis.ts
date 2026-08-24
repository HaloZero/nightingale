import { invoke, listen } from "./runtime";
import type { LibraryMenuFilters } from "@/types/LibraryMenuFilters";
import { ShiftDone } from "@/types/ShiftDone";
import type { SongTarget } from "@/types/SongTarget";

export const songsByHashes = (hashes: string[]): SongTarget => ({ kind: "hashes", hashes });
export const songsByFilter = (filters: LibraryMenuFilters): SongTarget => ({
  kind: "filter",
  filters,
});

export const enqueue = async (target: SongTarget): Promise<number> => {
  return await invoke<number>("enqueue", { target });
};

export const deleteSongCache = async (target: SongTarget): Promise<number> => {
  return await invoke<number>("delete_song_cache", { target });
};

export const reanalyzeTranscript = async (
  target: SongTarget,
  language?: string,
): Promise<number> => {
  return await invoke<number>("reanalyze_transcript", { target, language });
};

export const reanalyzeFull = async (target: SongTarget): Promise<number> => {
  return await invoke<number>("reanalyze_full", { target });
};

export const realign = async (target: SongTarget, language?: string): Promise<number> => {
  return await invoke<number>("realign", { target, language });
};

export const reanalyzeForceTranscribe = async (target: SongTarget): Promise<number> => {
  return await invoke<number>("reanalyze_force_transcribe", { target });
};

export const refreshMetadata = async (target: SongTarget): Promise<number> => {
  return await invoke<number>("refresh_metadata", { target });
};

export const shiftTempo = async (fileHash: string, tempo: number): Promise<void> => {
  return await invoke<void>("shift_tempo", { fileHash, tempo });
};

export const shiftKey = async (
  fileHash: string,
  key: string,
  pitchRatio: number,
  keyOffset: number,
): Promise<void> => {
  return await invoke<void>("shift_key", { fileHash, key, pitchRatio, keyOffset });
};

export const onShiftKeyDone = async (cb: (payload: ShiftDone) => void): Promise<() => void> => {
  return await listen<ShiftDone>("shift-key-done", ({ payload }) => cb(payload));
};

export const onShiftTempoDone = async (cb: (payload: ShiftDone) => void): Promise<() => void> => {
  return await listen<ShiftDone>("shift-tempo-done", ({ payload }) => cb(payload));
};
