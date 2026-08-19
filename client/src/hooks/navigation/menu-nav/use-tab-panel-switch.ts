import { useEffect } from "react";
import { blurActiveTextInput } from "./dom";
import type { MenuNavHookOptions } from "./types";

export function useTabPanelSwitch({ menuFocus, refs, lock }: MenuNavHookOptions) {
  const { activate, actionsRef, setFocus } = menuFocus;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Tab" || refs.overlayOpenRef.current) {
        return;
      }

      event.preventDefault();

      activate();
      lock.lockTemporarily();
      blurActiveTextInput();

      setFocus((prev) => {
        let panel = prev.panel;
        if (prev.panel === "sidebar") panel = "songList";
        else if (prev.panel === "songList" && actionsRef.current.hasSongDetails) {
          panel = "songDetails";
        } else panel = "sidebar";

        return {
          ...prev,
          active: true,
          actionsFocused: false,
          panel,
          source: "nav",
        };
      });
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [actionsRef, activate, lock, refs, setFocus]);
}
