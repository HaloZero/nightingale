import { z } from 'zod';

import type { AppConfig } from '@/types/AppConfig';
import type { CachePaths } from '@/types/CachePaths';
import type { ChromecastConfig } from '@/types/ChromecastConfig';
import type { LibrarySource } from '@/types/LibrarySource';
import type { Song } from '@/types/Song';
import type { SongOrigin } from '@/types/SongOrigin';
import type { SongsMeta } from '@/types/SongsMeta';
import type { UsdxBundle } from '@/types/UsdxBundle';

const nullableString = z.string().nullable();
const nullableNumber = z.number().nullable();
const nullableBoolean = z.boolean().nullable();

const cachePathsSchema: z.ZodType<CachePaths> = z.object({
  songs: nullableString,
  videos: nullableString,
  models: nullableString,
  vendor: nullableString,
});

const librarySourceSchema: z.ZodType<LibrarySource> = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('folder'), path: z.string() }),
  z.object({
    kind: z.literal('jellyfin'),
    base_url: z.string(),
    user_id: z.string(),
    username: z.string(),
    access_token: z.string(),
    device_id: z.string(),
    library_ids: z.array(z.string()),
  }),
  z.object({
    kind: z.literal('navidrome'),
    base_url: z.string(),
    username: z.string(),
    password: z.string(),
  }),
  z.object({
    kind: z.literal('plex'),
    base_url: z.string(),
    server_name: z.string(),
    machine_id: z.string(),
    username: z.string(),
    access_token: z.string(),
    client_id: z.string(),
    section_ids: z.array(z.string()),
  }),
]);

const chromecastConfigSchema: z.ZodType<ChromecastConfig> = z.object({
  host: z.string(),
  port: z.number(),
  server_base_url: nullableString,
  karaoke_video: z.boolean(),
});

export const appConfigSchema: z.ZodType<AppConfig> = z.object({
  data_path: nullableString,
  cache_paths: cachePathsSchema.nullable(),
  last_folder: nullableString,
  library_source: librarySourceSchema.nullable(),
  last_theme: nullableNumber,
  guide_volume: nullableNumber,
  fullscreen: nullableBoolean,
  playback_mode: nullableString,
  dark_mode: nullableBoolean,
  mic_active: nullableBoolean,
  mic_monitoring: nullableBoolean,
  mic_monitor_gain: nullableNumber,
  mic_latency_compensation_sec: nullableNumber,
  preferred_mic: nullableString,
  whisper_model: nullableString,
  beam_size: nullableNumber,
  batch_size: nullableNumber,
  last_video_flavor: nullableNumber,
  pixabay_video_rotation: nullableBoolean,
  lyrics_vertical_position: nullableString,
  lyrics_horizontal_position: nullableString,
  lyrics_scale: nullableNumber,
  pitch_graph_scale: nullableNumber,
  separator: nullableString,
  asr_engine: nullableString,
  align_backend: nullableString,
  alt_align_backend: nullableString,
  vocal_detection_threshold_pct: nullableNumber,
  use_external_lyrics: nullableBoolean,
  auto_analyze: nullableBoolean,
  restore_analyze: nullableBoolean,
  track_analysis_timings: nullableBoolean,
  refresh_lyrics_on_scan: nullableBoolean,
  song_list_view: nullableString,
  song_list_sort: z
    .array(
      z.object({
        column: z.enum(['title', 'artist', 'album', 'duration', 'status']),
        direction: z.enum(['ascending', 'descending']),
      }),
    )
    .nullable(),
  language_overrides: z.record(z.string(), z.string()).nullable(),
  parallel_analysis_enabled: nullableBoolean,
  parallel_analysis_url: nullableString,
  parallel_analysis_only: nullableBoolean,
  chromecast: chromecastConfigSchema.nullable(),
});

export const songsMetaSchema: z.ZodType<SongsMeta> = z.object({
  count: z.number(),
  folder: z.string(),
  processed_count: z.number(),
  songs_count: z.number(),
  videos_count: z.number(),
  analyzed_count: z.number(),
});

const usdxBundleSchema: z.ZodType<UsdxBundle> = z.object({
  txt_path: z.string(),
  audio: z.string(),
  vocals: nullableString,
  instrumental: nullableString,
  video: nullableString,
});

const songOriginSchema: z.ZodType<SongOrigin> = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('local_file') }),
  z.object({
    kind: z.literal('jellyfin'),
    item_id: z.string(),
    container: nullableString,
    cover_tag: nullableString,
  }),
  z.object({
    kind: z.literal('navidrome'),
    item_id: z.string(),
    container: nullableString,
    cover_tag: nullableString,
  }),
  z.object({
    kind: z.literal('plex'),
    item_id: z.string(),
    part_key: z.string(),
    container: nullableString,
    cover_tag: nullableString,
  }),
]);

export const songSchema: z.ZodType<Song> = z.object({
  path: z.string(),
  file_hash: z.string(),
  title: z.string(),
  artist: z.string(),
  album: z.string(),
  genre: z.string(),
  duration_secs: z.number(),
  album_art_path: nullableString,
  is_analyzed: z.boolean(),
  language: nullableString,
  transcript_source: z.enum(['Lyrics', 'Generated', 'Usdx', 'Lrc']).nullable(),
  align_backend: nullableString,
  key: nullableString,
  override_key: nullableString,
  tempo: z.number(),
  key_offset: z.number(),
  is_video: z.boolean(),
  usdx: usdxBundleSchema.nullable(),
  origin: songOriginSchema,
  no_stems: z.boolean(),
  has_lrc_file: z.boolean(),
  has_embedded_lyrics: z.boolean(),
  karaoke_video_version: z.number(),
  youtube_karaoke_video_version: z.number(),
});

export const webBootstrapSchema = z.object({
  config: appConfigSchema,
  songsMeta: songsMetaSchema,
  dataPathPinned: z.boolean().optional(),
  libraryPinned: z.boolean().optional(),
});

export const playbackLocationStateSchema = z.object({
  song: songSchema,
  queuePlayback: z.boolean().optional(),
  playbackId: z.string().optional(),
});
