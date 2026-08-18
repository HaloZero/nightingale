import { invoke, listen } from "./runtime";
import type { LibraryMenuFilters } from "@/types/LibraryMenuFilters";
import { ShiftDone } from "@/types/ShiftDone";

export const enqueueOne = async (fileHash: string): Promise<void> => {
  return await invoke<void>("enqueue_one", { fileHash });
};

export const enqueueAll = async (filters: LibraryMenuFilters): Promise<void> => {
  return await invoke<void>("enqueue_all", { filters });
};

export const deleteSongCache = async (fileHash: string): Promise<void> => {
  return await invoke<void>("delete_song_cache", { fileHash });
};

export const deleteSongCacheAll = async (filters: LibraryMenuFilters): Promise<number> => {
  return await invoke<number>("delete_song_cache_all", { filters });
};

export const reanalyzeTranscript = async (fileHash: string, language?: string): Promise<void> => {
  return await invoke<void>("reanalyze_transcript", { fileHash, language });
};

export const reanalyzeFull = async (fileHash: string): Promise<void> => {
  return await invoke<void>("reanalyze_full", { fileHash });
};

export const realign = async (fileHash: string, language?: string): Promise<void> => {
  return await invoke<void>("realign", { fileHash, language });
};

export const reanalyzeForceTranscribe = async (fileHash: string): Promise<void> => {
  return await invoke<void>("reanalyze_force_transcribe", { fileHash });
};

export const reanalyzeAllFull = async (filters: LibraryMenuFilters): Promise<number> => {
  return await invoke<number>("reanalyze_all_full", { filters });
};

export const reanalyzeAllTranscript = async (
  filters: LibraryMenuFilters,
  language?: string,
): Promise<number> => {
  return await invoke<number>("reanalyze_all_transcript", { filters, language });
};

export const reanalyzeAllForceTranscribe = async (filters: LibraryMenuFilters): Promise<number> => {
  return await invoke<number>("reanalyze_all_force_transcribe", { filters });
};

export const realignAll = async (
  filters: LibraryMenuFilters,
  language?: string,
): Promise<number> => {
  return await invoke<number>("realign_all", { filters, language });
};

export const refreshMetadata = async (fileHash: string): Promise<boolean> => {
  return await invoke<boolean>("refresh_metadata", { fileHash });
};

export const refreshMetadataAll = async (filters: LibraryMenuFilters): Promise<number> => {
  return await invoke<number>("refresh_metadata_all", { filters });
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
