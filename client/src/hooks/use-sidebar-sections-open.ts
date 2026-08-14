import { atom, useAtom } from "jotai";
import type { LibraryMenuSection } from "@/lib/library-menu-filter";

const INITIAL_SECTIONS_OPEN: Record<LibraryMenuSection, boolean> = {
  hot: true,
  no_metadata: false,
  artists: false,
  albums: false,
  playlists: false,
  languages: false,
};

const sidebarSectionsOpenAtom = atom<Record<LibraryMenuSection, boolean>>(INITIAL_SECTIONS_OPEN);

export function useSidebarSectionsOpen() {
  return useAtom(sidebarSectionsOpenAtom);
}
