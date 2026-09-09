import type { LibraryMenuFilters } from '@/types/LibraryMenuFilters';
import type { LibraryMenuItem } from '@/types/LibraryMenuItem';

export type LibraryMenuSection =
  | 'hot'
  | 'no_metadata'
  | 'lyrics'
  | 'karaoke_video'
  | 'artists'
  | 'albums'
  | 'genres'
  | 'playlists'
  | 'languages';

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
  queued: { ...EMPTY_LIBRARY_FILTER, query: 'queued' },
  analysed: { ...EMPTY_LIBRARY_FILTER, query: 'analysed' },
  videos: { ...EMPTY_LIBRARY_FILTER, query: 'videos' },
  usdx: { ...EMPTY_LIBRARY_FILTER, query: 'usdx' },
};

const NO_METADATA_FILTERS: Record<string, LibraryMenuFilters> = {
  unknown_artist: { ...EMPTY_LIBRARY_FILTER, artist: 'unknown_artist' },
  unknown_album: { ...EMPTY_LIBRARY_FILTER, album: 'unknown_album' },
};

const LYRICS_FILTERS: Record<string, LibraryMenuFilters> = {
  has_external_lyrics: { ...EMPTY_LIBRARY_FILTER, query: 'has_external_lyrics' },
  no_external_lyrics: { ...EMPTY_LIBRARY_FILTER, query: 'no_external_lyrics' },
};

const KARAOKE_VIDEO_FILTERS: Record<string, LibraryMenuFilters> = {
  has_karaoke_video: { ...EMPTY_LIBRARY_FILTER, query: 'has_karaoke_video' },
  has_karaoke_video_outdated: { ...EMPTY_LIBRARY_FILTER, query: 'has_karaoke_video_outdated' },
  has_youtube_karaoke_video: { ...EMPTY_LIBRARY_FILTER, query: 'has_youtube_karaoke_video' },
  has_youtube_karaoke_video_outdated: {
    ...EMPTY_LIBRARY_FILTER,
    query: 'has_youtube_karaoke_video_outdated',
  },
};

// Sections backed by a lookup table of pre-built filters (see `HOT_FILTERS`
// and friends above), keyed by the selected item's value.
const LOOKUP_TABLE_SECTIONS: Partial<
  Record<LibraryMenuSection, Record<string, LibraryMenuFilters>>
> = {
  hot: HOT_FILTERS,
  no_metadata: NO_METADATA_FILTERS,
  lyrics: LYRICS_FILTERS,
  karaoke_video: KARAOKE_VIDEO_FILTERS,
};

// Sections that filter directly on one `LibraryMenuFilters` field, set to
// the selected item's value.
const DIRECT_FIELD_SECTIONS: Partial<Record<LibraryMenuSection, keyof LibraryMenuFilters>> = {
  artists: 'artist',
  albums: 'album',
  genres: 'genre',
  playlists: 'playlist',
  languages: 'language',
};

export function libraryFilterFromMenuSelection(
  section: LibraryMenuSection,
  item: LibraryMenuItem,
): LibraryMenuFilters {
  const table = LOOKUP_TABLE_SECTIONS[section];
  if (table) {
    return table[item.value] ?? EMPTY_LIBRARY_FILTER;
  }

  const field = DIRECT_FIELD_SECTIONS[section];
  if (field) {
    return { ...EMPTY_LIBRARY_FILTER, [field]: item.value };
  }

  return EMPTY_LIBRARY_FILTER;
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
