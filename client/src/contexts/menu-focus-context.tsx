import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type RefObject,
  type ReactNode,
} from "react";

export type FocusPanel = "songList" | "sidebar" | "songDetails";
export type FocusSource = "mouse" | "nav";

export interface MenuFocus {
  active: boolean;
  panel: FocusPanel;
  songIndex: number;
  sidebarIndex: number;
  sidebarSubIndex: number;
  actionsFocused: boolean;
  source: FocusSource;
}

export interface MenuFocusActions {
  onConfirmSong: ((index: number) => void) | null;
  onConfirmSidebar: ((index: number) => void) | null;
  onConfirmActions: (() => void) | null;
  onSidebarBack: (() => boolean) | null;
  isSidebarBusy: (() => boolean) | null;
  hasSongDetails: boolean;
  songCount: number;
  sidebarCount: number;
  sidebarSubCountByIndex: Map<number, number>;
}

export interface MenuFocusContextValue {
  focus: MenuFocus;
  setFocus: (updater: (prev: MenuFocus) => MenuFocus) => void;
  activate: () => void;
  deactivate: () => void;
  actionsRef: RefObject<MenuFocusActions>;
  scrollRef: RefObject<HTMLElement | null>;
  scrollTopRef: RefObject<number>;
  sidebarScrollRef: RefObject<HTMLElement | null>;
  sidebarScrollTopRef: RefObject<number>;
  selectedSongKeyRef: RefObject<string | null>;
}

const MenuFocusContext = createContext<MenuFocusContextValue | null>(null);

const INITIAL_FOCUS: MenuFocus = {
  active: false,
  panel: "songList",
  songIndex: 0,
  sidebarIndex: 0,
  sidebarSubIndex: 0,
  actionsFocused: false,
  source: "nav",
};

const INITIAL_ACTIONS: MenuFocusActions = {
  onConfirmSong: null,
  onConfirmSidebar: null,
  onConfirmActions: null,
  onSidebarBack: null,
  isSidebarBusy: null,
  hasSongDetails: false,
  songCount: 0,
  sidebarCount: 0,
  sidebarSubCountByIndex: new Map(),
};

export function MenuFocusProvider({ children }: { children: ReactNode }) {
  const [focus, setFocusState] = useState<MenuFocus>(INITIAL_FOCUS);
  const actionsRef = useRef<MenuFocusActions>({ ...INITIAL_ACTIONS });
  const scrollRef = useRef<HTMLElement | null>(null);
  const scrollTopRef = useRef<number>(0);
  const sidebarScrollRef = useRef<HTMLElement | null>(null);
  const sidebarScrollTopRef = useRef<number>(0);
  const selectedSongKeyRef = useRef<string | null>(null);

  const setFocus = useCallback((updater: (prev: MenuFocus) => MenuFocus) => {
    setFocusState(updater);
  }, []);

  const activate = useCallback(() => {
    setFocusState((prev) => (prev.active ? prev : { ...prev, active: true }));
  }, []);

  const deactivate = useCallback(() => {
    setFocusState((prev) => (prev.active ? { ...prev, active: false } : prev));
  }, []);

  const value = useMemo(
    () => ({
      focus,
      setFocus,
      activate,
      deactivate,
      actionsRef,
      scrollRef,
      scrollTopRef,
      sidebarScrollRef,
      sidebarScrollTopRef,
      selectedSongKeyRef,
    }),
    [focus, setFocus, activate, deactivate],
  );

  return <MenuFocusContext.Provider value={value}>{children}</MenuFocusContext.Provider>;
}

export function useMenuFocus(): MenuFocusContextValue {
  const ctx = useContext(MenuFocusContext);
  if (!ctx) {
    throw new Error("useMenuFocus must be used within MenuFocusProvider");
  }
  return ctx;
}
