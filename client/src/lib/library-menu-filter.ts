import type { LibraryMenuItem } from "@/types/LibraryMenuItem";
import type { LibraryMenuFilters } from "@/types/LibraryMenuFilters";

export type LibraryMenuSection =
  | "hot"
  | "no_metadata"
  | "lyrics"
  | "karaoke_video"
  | "artists"
  | "albums"
  | "genres"
  | "playlists"
  | "languages";

export const EMPTY_LIBRARY_FILTER: LibraryMenuFilters = {
  artist: null,
  album: null,
  genre: null,
  playlist: null,
  query: null,
  status: null,
  transcript_source: null,
  search: null,
  language: null,
};

const HOT_FILTERS: Record<string, LibraryMenuFilters> = {
  all: { ...EMPTY_LIBRARY_FILTER },
  queued: { ...EMPTY_LIBRARY_FILTER, query: "queued" },
  analysed: { ...EMPTY_LIBRARY_FILTER, query: "analysed" },
  videos: { ...EMPTY_LIBRARY_FILTER, query: "videos" },
  usdx: { ...EMPTY_LIBRARY_FILTER, query: "usdx" },
};

const NO_METADATA_FILTERS: Record<string, LibraryMenuFilters> = {
  unknown_artist: { ...EMPTY_LIBRARY_FILTER, artist: "unknown_artist" },
  unknown_album: { ...EMPTY_LIBRARY_FILTER, album: "unknown_album" },
};

const LYRICS_FILTERS: Record<string, LibraryMenuFilters> = {
  has_external_lyrics: { ...EMPTY_LIBRARY_FILTER, query: "has_external_lyrics" },
  no_external_lyrics: { ...EMPTY_LIBRARY_FILTER, query: "no_external_lyrics" },
};

const KARAOKE_VIDEO_FILTERS: Record<string, LibraryMenuFilters> = {
  has_karaoke_video_v1: { ...EMPTY_LIBRARY_FILTER, query: "has_karaoke_video_v1" },
  has_karaoke_video_v2: { ...EMPTY_LIBRARY_FILTER, query: "has_karaoke_video_v2" },
  has_youtube_karaoke_video_v1: { ...EMPTY_LIBRARY_FILTER, query: "has_youtube_karaoke_video_v1" },
  has_youtube_karaoke_video_v2: { ...EMPTY_LIBRARY_FILTER, query: "has_youtube_karaoke_video_v2" },
};

export function libraryFilterFromMenuSelection(
  section: LibraryMenuSection,
  item: LibraryMenuItem,
): LibraryMenuFilters {
  switch (section) {
    case "hot":
      return HOT_FILTERS[item.value] ?? EMPTY_LIBRARY_FILTER;
    case "no_metadata":
      return NO_METADATA_FILTERS[item.value] ?? EMPTY_LIBRARY_FILTER;
    case "lyrics":
      return LYRICS_FILTERS[item.value] ?? EMPTY_LIBRARY_FILTER;
    case "karaoke_video":
      return KARAOKE_VIDEO_FILTERS[item.value] ?? EMPTY_LIBRARY_FILTER;
    case "artists":
      return { ...EMPTY_LIBRARY_FILTER, artist: item.value };
    case "albums":
      return { ...EMPTY_LIBRARY_FILTER, album: item.value };
    case "genres":
      return { ...EMPTY_LIBRARY_FILTER, genre: item.value };
    case "playlists":
      return { ...EMPTY_LIBRARY_FILTER, playlist: item.value };
    case "languages":
      return { ...EMPTY_LIBRARY_FILTER, language: item.value };
  }
}

export function libraryFiltersEqual(a: LibraryMenuFilters, b: LibraryMenuFilters): boolean {
  return (
    a.artist === b.artist &&
    a.album === b.album &&
    a.genre === b.genre &&
    a.playlist === b.playlist &&
    a.query === b.query &&
    a.language === b.language
  );
}

export function isLibraryMenuItemActive(
  section: LibraryMenuSection,
  item: LibraryMenuItem,
  current: LibraryMenuFilters,
): boolean {
  return libraryFiltersEqual(current, libraryFilterFromMenuSelection(section, item));
}
