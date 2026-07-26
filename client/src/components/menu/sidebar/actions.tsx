import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar";
import { EXIT_SUPPORTED } from "@/bridge/exit";
import { useMenuFocus } from "@/contexts/menu-focus-context";
import { useCurrentProfile } from "@/hooks/use-current-profile";
import { useDialog } from "@/hooks/use-dialog";
import { useDonationSeen } from "@/hooks/use-donation-seen";
import { useShouldRunSetup } from "@/hooks/use-should-run-setup";
import {
  ChevronsUpDownIcon,
  CogIcon,
  DoorOpenIcon,
  DownloadIcon,
  HeartIcon,
  InfoIcon,
  RefreshCcwDotIcon,
  TrophyIcon,
  UserIcon,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { useNavInput } from "@/hooks/navigation/use-nav-input";
import { useUpdate } from "@/queries/use-update";
import { useNavigate } from "react-router";
import { useIsMobile } from "@/hooks/use-is-mobile";

interface ActionsProps {
  registerCallback: (callback: (() => void) | null) => void;
  focusedSidebarIndex: number;
}

export const Actions = ({ registerCallback, focusedSidebarIndex }: ActionsProps) => {
  const { setMode } = useDialog();
  const { setOpen } = useSidebar();
  const navigate = useNavigate();
  const profile = useCurrentProfile();
  const { focus, actionsRef } = useMenuFocus();
  const { setShouldRunSetup } = useShouldRunSetup();

  const update = useUpdate();
  const updateAvailable = update.status === "available";

  const { seen: donationSeen } = useDonationSeen();
  const showDonationBadge = !donationSeen;
  const showDonationAvatarBadge = showDonationBadge && !updateAvailable;

  let avatarBadge: ReactNode = null;
  if (updateAvailable) {
    avatarBadge = (
      <span
        aria-label="Update available"
        className="absolute -right-0.5 -top-0.5 size-2 rounded-full bg-chart-3 ring-2 ring-sidebar"
      />
    );
  } else if (showDonationAvatarBadge) {
    avatarBadge = (
      <span
        aria-label="Support Nightingale"
        className="absolute -right-0.5 -top-0.5 size-2 rounded-full bg-pink-500 ring-2 ring-sidebar"
      />
    );
  }

  const [dropdownOpen, setDropdownOpen] = useState(false);
  const isMobile = useIsMobile();
  const dropdownOpenRef = useRef(false);

  dropdownOpenRef.current = dropdownOpen;

  useEffect(() => {
    registerCallback(() => {
      setDropdownOpen(true);

      setTimeout(() => {
        const firstItem = document.querySelector('[role="menu"] [role="menuitem"]');

        if (firstItem instanceof HTMLElement) {
          firstItem.focus();
        }
      }, 50);
    });

    actionsRef.current.onSidebarBack = () => {
      if (dropdownOpenRef.current) {
        setDropdownOpen(false);

        return true;
      }
      return false;
    };

    actionsRef.current.isSidebarBusy = () => dropdownOpenRef.current;

    return () => {
      registerCallback(null);

      actionsRef.current.onSidebarBack = null;
      actionsRef.current.isSidebarBusy = null;
    };
  }, [registerCallback, actionsRef]);

  useNavInput(
    useCallback((action) => {
      if (!dropdownOpenRef.current) {
        return;
      }

      const focused = document.activeElement;

      if (action.up || action.down) {
        const key = action.up ? "ArrowUp" : "ArrowDown";

        if (focused) {
          focused.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
        }
      }

      if (action.confirm) {
        if (focused instanceof HTMLElement) {
          focused.click();
        }
      }
    }, []),
  );

  const isSidebarActive = focus.active && focus.panel === "sidebar";

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu open={dropdownOpen} onOpenChange={setDropdownOpen}>
          <DropdownMenuTrigger asChild>
            <SidebarMenuButton
              tabIndex={-1}
              size="lg"
              data-sidebar-nav-index={focusedSidebarIndex}
              className={`data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground hover:ring-primary ${
                isSidebarActive && focus.sidebarIndex === focusedSidebarIndex
                  ? "ring-2 ring-primary bg-sidebar-accent"
                  : ""
              }`}
            >
              <div className="relative">
                <Avatar>
                  <AvatarFallback>
                    {profile ? profile.slice(0, 2).toLocaleUpperCase() : "NP"}
                  </AvatarFallback>
                </Avatar>
                {avatarBadge}
              </div>
              <span className="truncate font-medium">{profile ?? "No Selected Profile"}</span>
              <ChevronsUpDownIcon className="ml-auto size-4" />
            </SidebarMenuButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            side={isMobile ? "top" : "right"}
            align={isMobile ? "start" : "end"}
            collisionPadding={8}
            className="min-w-56"
          >
            <DropdownMenuLabel>Setup</DropdownMenuLabel>
            <DropdownMenuGroup>
              <DropdownMenuItem onClick={() => setShouldRunSetup(true)}>
                <RefreshCcwDotIcon />
                Re-run Setup
              </DropdownMenuItem>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>General</DropdownMenuLabel>
            <DropdownMenuGroup>
              <DropdownMenuItem
                onClick={() => {
                  if (profile) {
                    return setMode("select-profile");
                  }

                  setMode("create-profile");
                }}
              >
                <UserIcon />
                Profile
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setMode("leaderboards")}>
                <TrophyIcon />
                Leaderboards
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => {
                  setDropdownOpen(false);
                  if (isMobile) {
                    setOpen(false);
                  }
                  navigate("/settings");
                }}
              >
                <CogIcon />
                Settings
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setMode("update")}>
                <DownloadIcon />
                <span>Update</span>
                {updateAvailable && (
                  <span
                    aria-label="Update available"
                    className="ml-auto size-2 rounded-full bg-chart-3"
                  />
                )}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setMode("donate")}>
                <HeartIcon />
                <span>Donate</span>
                {showDonationBadge && (
                  <span
                    aria-label="Support Nightingale"
                    className="ml-auto size-2 rounded-full bg-pink-500"
                  />
                )}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setMode("about")}>
                <InfoIcon />
                About
              </DropdownMenuItem>
              {EXIT_SUPPORTED && (
                <DropdownMenuItem onClick={() => setMode("exit")}>
                  <DoorOpenIcon />
                  Exit
                </DropdownMenuItem>
              )}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
};
