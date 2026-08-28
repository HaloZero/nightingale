import type { MediaEndpoint } from "@/types/MediaEndpoint";
import type { AudioPaths } from "@/types/Transcript";
import type { Transcript } from "@/types/Transcript";
import type { YoutubeBackground } from "@/types/YoutubeBackground";
import { invoke, listen, type UnlistenFn } from "./runtime";

export const loadTranscript = async (fileHash: string): Promise<Transcript> => {
  return await invoke<Transcript>("load_transcript", { fileHash });
};

export const getAudioPaths = async (fileHash: string): Promise<AudioPaths> => {
  return await invoke<AudioPaths>("get_audio_paths", { fileHash });
};

export const ensureMp3Stems = (fileHash: string): void => {
  void invoke<void>("ensure_mp3_stems", { fileHash });
};

export const ensurePlayableSourceVideo = async (fileHash: string): Promise<string | null> => {
  return await invoke<string | null>("ensure_playable_source_video", { fileHash });
};

export const loadYoutubeBackground = async (
  fileHash: string,
): Promise<YoutubeBackground | null> => {
  return await invoke<YoutubeBackground | null>("load_youtube_background", { fileHash });
};

export interface StemsReadyEvent {
  file_hash: string;
  error: string | null;
}

export const onStemsReady = async (cb: (event: StemsReadyEvent) => void): Promise<UnlistenFn> => {
  return await listen<StemsReadyEvent>("stems-ready", ({ payload }) => cb(payload));
};

export const fetchPixabayVideos = async (flavor: string): Promise<string[]> => {
  return await invoke<string[]>("fetch_pixabay_videos", { flavor });
};

export const getMediaEndpoint = async (): Promise<MediaEndpoint> => {
  return await invoke<MediaEndpoint>("get_media_endpoint");
};

export interface PixabayVideoDownloaded {
  flavor: string;
  path: string;
  evictedPath?: string;
}

export const onPixabayVideoDownloaded = async (
  cb: (event: PixabayVideoDownloaded) => void,
): Promise<UnlistenFn> => {
  return await listen<{ flavor: string; path: string; evicted_path: string | null }>(
    "pixabay-video-downloaded",
    ({ payload }) =>
      cb({
        flavor: payload.flavor,
        path: payload.path,
        evictedPath: payload.evicted_path ?? undefined,
      }),
  );
};
