import { atom, useAtom } from "jotai";
import { EMPTY_LIBRARY_FILTER } from "@/lib/library-menu-filter";
import type { LibraryMenuFilters } from "@/types/LibraryMenuFilters";

export const libraryFilterAtom = atom<LibraryMenuFilters>(EMPTY_LIBRARY_FILTER);

export const useLibraryFilter = () => {
  const [filter, setLibraryFilter] = useAtom(libraryFilterAtom);

  return {
    artist: filter.artist,
    album: filter.album,
    playlist: filter.playlist,
    query: filter.query,
    status: filter.status,
    transcript_source: filter.transcript_source,
    search: filter.search,
    language: filter.language,
    setLibraryFilter,
  };
};

export type { LibraryMenuFilters } from "@/types/LibraryMenuFilters";
export type { LibraryMenuSection } from "@/lib/library-menu-filter";
