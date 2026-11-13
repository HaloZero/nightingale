import type { LibraryMenuFilters } from '@/types/LibraryMenuFilters';
import type { LibraryMenuItem } from '@/types/LibraryMenuItem';

export type LibraryMenuSection =
  | 'hot'
  | 'no_metadata'
  | 'artists'
  | 'albums'
  | 'genres'
  | 'playlists'
  | 'languages'
  | 'folders';

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
  folder: null,
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

// Sections whose selection narrows by setting exactly one filter field to
// the clicked item's value -- `hot`/`no_metadata` are handled separately
// above since they map to a fixed lookup table instead of the item value.
const SINGLE_FIELD_SECTIONS: Partial<Record<LibraryMenuSection, keyof LibraryMenuFilters>> = {
  artists: 'artist',
  albums: 'album',
  genres: 'genre',
  playlists: 'playlist',
  languages: 'language',
  folders: 'folder',
};

export function libraryFilterFromMenuSelection(
  section: LibraryMenuSection,
  item: LibraryMenuItem,
): LibraryMenuFilters {
  if (section === 'hot') {
    return HOT_FILTERS[item.value] ?? EMPTY_LIBRARY_FILTER;
  }
  if (section === 'no_metadata') {
    return NO_METADATA_FILTERS[item.value] ?? EMPTY_LIBRARY_FILTER;
  }
  const field = SINGLE_FIELD_SECTIONS[section];
  return field ? { ...EMPTY_LIBRARY_FILTER, [field]: item.value } : EMPTY_LIBRARY_FILTER;
}

export function libraryFiltersEqual(a: LibraryMenuFilters, b: LibraryMenuFilters): boolean {
  return (
    a.artist === b.artist &&
    a.album === b.album &&
    a.genre === b.genre &&
    a.playlist === b.playlist &&
    a.folder === b.folder &&
    a.language === b.language &&
    a.query === b.query
  );
}

export function isLibraryMenuItemActive(
  section: LibraryMenuSection,
  item: LibraryMenuItem,
  current: LibraryMenuFilters,
): boolean {
  return libraryFiltersEqual(current, libraryFilterFromMenuSelection(section, item));
}
