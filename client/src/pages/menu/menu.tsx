import { ClearCacheDialog } from "@/components/menu/dialogs/clear-cache";
import { DonateDialog } from "@/components/menu/dialogs/donate";
import { EditLyricsDialog } from "@/components/menu/dialogs/edit-lyrics";
import { ExitDialog } from "@/components/menu/dialogs/exit";
import { InfoDialog } from "@/components/menu/dialogs/info";
import { LeaderboardsDialog } from "@/components/menu/dialogs/leaderboards";
import { JellyfinConnectDialog } from "@/components/menu/dialogs/remote-source/jellyfin-connect";
import { NavidromeConnectDialog } from "@/components/menu/dialogs/remote-source/navidrome-connect";
import { PlexConnectDialog } from "@/components/menu/dialogs/remote-source/plex-connect";
import { SelectLanguageDialog } from "@/components/menu/dialogs/language";
import { CreateProfileDialog } from "@/components/menu/dialogs/profile/create";
import { SelectProfileDialog } from "@/components/menu/dialogs/profile/select";
import { Setup } from "@/components/menu/dialogs/setup";
import { UpdateDialog } from "@/components/menu/dialogs/update";
import { Sidebar } from "@/components/menu/sidebar/sidebar";
import { EmptySongList } from "@/components/menu/song-list/empty-song-list";
import { SongList } from "@/components/menu/song-list/song-list";
import { SidebarInset } from "@/components/ui/sidebar";
import { EXIT_SUPPORTED } from "@/bridge/exit";
import { useMenuNav } from "@/hooks/navigation/use-menu-nav";
import { useDialog } from "@/hooks/use-dialog";
import { useShouldRunSetup } from "@/hooks/use-should-run-setup";
import { useSongsMeta } from "@/queries/use-songs";
import { useCallback } from "react";
import { Outlet, useLocation } from "react-router";

export const MenuIndex = () => {
  const { data: meta, isLoading: isLoadingMeta } = useSongsMeta();

  if (isLoadingMeta) {
    return null;
  }

  if (meta?.folder) {
    return <SongList />;
  }

  return <EmptySongList />;
};

export const MenuLayout = () => {
  const { mode, setMode } = useDialog();
  const { shouldRunSetup } = useShouldRunSetup();
  const location = useLocation();

  const isContentPage = location.pathname !== "/";
  const overlayOpen = isContentPage || mode !== null || shouldRunSetup;

  const onBack = useCallback(() => {
    setMode((prev) => {
      if (prev === null) {
        // Web mode has no app to exit; swallow the back input rather than
        // surfacing a dialog whose confirm action can't do anything useful.
        return EXIT_SUPPORTED ? "exit" : null;
      }

      if (prev === "exit") {
        return null;
      }

      return prev;
    });
  }, [setMode]);

  useMenuNav({ overlayOpen, onBack });

  return (
    <Sidebar>
      {EXIT_SUPPORTED && <ExitDialog />}
      <CreateProfileDialog />
      <SelectProfileDialog />
      <InfoDialog />
      <LeaderboardsDialog />
      <UpdateDialog />
      <DonateDialog />
      <SelectLanguageDialog />
      <EditLyricsDialog />
      <ClearCacheDialog />
      <JellyfinConnectDialog />
      <NavidromeConnectDialog />
      <PlexConnectDialog />
      <Setup />
      <SidebarInset>
        <Outlet />
      </SidebarInset>
    </Sidebar>
  );
};
