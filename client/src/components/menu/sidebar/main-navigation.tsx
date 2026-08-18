import {
  SidebarContent,
  SidebarGroup,
  SidebarMenu,
  SidebarMenuSkeleton,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubItem,
  useSidebar,
} from "@/components/ui/sidebar";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";
import {
  ChevronDown,
  Flame,
  FileQuestionMark,
  DiscIcon,
  LanguagesIcon,
  ListMusicIcon,
  TagIcon,
  UserIcon,
  type LucideIcon,
} from "lucide-react";
import { useLibraryMenuItems } from "@/queries/use-library-menu-items";
import type { LibraryMenuItem } from "@/types/LibraryMenuItem";
import { Fragment, useCallback, useEffect, useMemo } from "react";
import { Badge } from "@/components/ui/badge";
import {
  isLibraryMenuItemActive,
  libraryFilterFromMenuSelection,
  type LibraryMenuSection,
} from "@/lib/library-menu-filter";
import type { LibraryMenuFilters } from "@/types/LibraryMenuFilters";
import { useLibraryFilter } from "@/hooks/use-library-filter";
import { usePersistentScroll } from "@/hooks/use-persistent-scroll";
import { useSidebarSectionsOpen } from "@/hooks/use-sidebar-sections-open";
import { useMenuFocus } from "@/contexts/menu-focus-context";
import {
  SidebarNavProvider,
  useSidebarRowFocus,
  type SidebarNavRow,
} from "@/contexts/sidebar-nav-context";
import { FolderActions } from "./folder-actions";
import { useNavigate } from "react-router";
import { useIsMobile } from "@/hooks/use-is-mobile";
import { ANALYSIS_STATUS_STYLES } from "@/lib/analysis-status-styles";

type NavSectionConfig = {
  section: LibraryMenuSection;
  label: string;
  icon: LucideIcon;
};

const NAV_SECTIONS: NavSectionConfig[] = [
  { section: "hot", label: "Quick Filters", icon: Flame },
  { section: "no_metadata", label: "No Metadata", icon: FileQuestionMark },
  { section: "artists", label: "Artists", icon: UserIcon },
  { section: "albums", label: "Albums", icon: DiscIcon },
  { section: "genres", label: "Genres", icon: TagIcon },
  { section: "languages", label: "Languages", icon: LanguagesIcon },
  { section: "playlists", label: "Playlists", icon: ListMusicIcon },
];

interface MenuItemCountsProps {
  item: LibraryMenuItem;
}

function MenuItemCounts({ item }: MenuItemCountsProps) {
  const segments = [
    ...(item.count > 0n
      ? [{ label: "total", value: item.count, className: "bg-muted text-muted-foreground" }]
      : []),
    ...(item.queuedCount > 0n
      ? [
          {
            label: "queued",
            value: item.queuedCount,
            className: ANALYSIS_STATUS_STYLES.queued,
          },
        ]
      : []),
    ...(item.analysingCount > 0n
      ? [
          {
            label: "analysing",
            value: item.analysingCount,
            className: ANALYSIS_STATUS_STYLES.analysing,
          },
        ]
      : []),
    ...(item.analysedCount > 0n
      ? [
          {
            label: "analysed",
            value: item.analysedCount,
            className: ANALYSIS_STATUS_STYLES.analysed,
          },
        ]
      : []),
  ];

  if (segments.length === 0) {
    return null;
  }

  return (
    <Badge
      className="h-[1.125rem] shrink-0 gap-0 overflow-hidden border border-foreground/15 bg-transparent p-0 text-[0.55rem] font-medium leading-none"
      title={segments.map(({ label, value }) => `${value} ${label}`).join(", ")}
    >
      {segments.map(({ label, value, className }, index) => (
        <Fragment key={label}>
          {index > 0 && <span className="h-full w-px bg-foreground/15" />}
          <span className={`${className} flex h-full min-w-5 items-center justify-center px-1`}>
            {value.toString()}
          </span>
        </Fragment>
      ))}
    </Badge>
  );
}

interface LibraryNavSubItemProps {
  section: LibraryMenuSection;
  item: LibraryMenuItem;
  filter: LibraryMenuFilters;
  onSelectItem: (section: LibraryMenuSection, item: LibraryMenuItem) => void;
}

function LibraryNavSubItem({ section, item, filter, onSelectItem }: LibraryNavSubItemProps) {
  const { isSidebarActive, isItemFocused, itemIndex } = useSidebarRowFocus(section, item.value);
  return (
    <SidebarMenuSubItem>
      <SidebarMenuButton
        data-sidebar-nav-index={itemIndex}
        isActive={isLibraryMenuItemActive(section, item, filter)}
        className={`flex h-fit items-center justify-between gap-2 px-2 py-1.5 hover:ring-primary ${
          isSidebarActive && isItemFocused ? "ring-2 ring-primary bg-sidebar-accent" : ""
        }`}
        onClick={() => onSelectItem(section, item)}
      >
        {item.label}
        <MenuItemCounts item={item} />
      </SidebarMenuButton>
    </SidebarMenuSubItem>
  );
}

interface LibraryNavSectionProps extends NavSectionConfig {
  items: LibraryMenuItem[];
  filter: LibraryMenuFilters;
  open: boolean;
  onToggleOpen: (open: boolean) => void;
  onSelectItem: (section: LibraryMenuSection, item: LibraryMenuItem) => void;
}

function LibraryNavSection({
  section,
  label,
  icon: Icon,
  items,
  filter,
  open,
  onToggleOpen,
  onSelectItem,
}: LibraryNavSectionProps) {
  const { isSidebarActive, isCollapseFocused, collapseIndex } = useSidebarRowFocus(section);

  return (
    <Collapsible open={open} onOpenChange={onToggleOpen} className="group/collapsible">
      <SidebarMenuItem>
        <CollapsibleTrigger asChild>
          <SidebarMenuButton
            data-sidebar-nav-index={collapseIndex}
            className={`flex w-full justify-between hover:ring-primary ${
              isSidebarActive && isCollapseFocused ? "ring-2 ring-primary bg-sidebar-accent" : ""
            }`}
          >
            <span className="flex items-center gap-2">
              <Icon className="size-4 shrink-0" />
              {label}
            </span>
            <ChevronDown className="transition-transform group-data-[state=open]/collapsible:rotate-180" />
          </SidebarMenuButton>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <SidebarMenuSub className="mr-0 pr-0">
            {items.map((item) => (
              <LibraryNavSubItem
                key={`${section}:${item.value}`}
                section={section}
                item={item}
                filter={filter}
                onSelectItem={onSelectItem}
              />
            ))}
          </SidebarMenuSub>
        </CollapsibleContent>
      </SidebarMenuItem>
    </Collapsible>
  );
}

interface MainNavigationProps {
  baseIndex: number;
  registerCallbacks: (callbacks: (() => void)[]) => void;
  folderFocusedSidebarIndex: number;
  registerFolderCallback: (callback: ((subIndex: number) => void) | null) => void;
}

export const MainNavigation = ({
  baseIndex,
  registerCallbacks,
  folderFocusedSidebarIndex,
  registerFolderCallback,
}: MainNavigationProps) => {
  const { data: menu } = useLibraryMenuItems();
  const { setOpen } = useSidebar();
  const isMobile = useIsMobile();
  const { setLibraryFilter, ...filter } = useLibraryFilter();
  const { focus } = useMenuFocus();
  const navigate = useNavigate();
  const { setScrollContainer } = usePersistentScroll("sidebar");
  const [openBySection, setOpenBySection] = useSidebarSectionsOpen();

  const selectMenuItem = useCallback(
    (section: LibraryMenuSection, item: LibraryMenuItem) => {
      setLibraryFilter((current) => ({
        ...libraryFilterFromMenuSelection(section, item),
        status: current.status,
        transcript_source: current.transcript_source,
      }));
      if (isMobile) {
        setOpen(false);
      }
      navigate("/");
    },
    [isMobile, navigate, setLibraryFilter, setOpen],
  );

  const visibleSections = useMemo(() => {
    if (!menu) {
      return [];
    }

    return NAV_SECTIONS.map((config) => ({
      ...config,
      visibleItems: menu[config.section].filter(({ count }) => count > 0n),
    })).filter(({ visibleItems }) => visibleItems.length > 0);
  }, [menu]);

  const rows = useMemo<SidebarNavRow[]>(() => {
    return visibleSections.flatMap(({ section, visibleItems }) => {
      const sectionRows: SidebarNavRow[] = [{ kind: "collapse", section }];
      if (openBySection[section]) {
        sectionRows.push(
          ...visibleItems.map(({ value }) => ({ kind: "item" as const, section, value })),
        );
      }
      return sectionRows;
    });
  }, [visibleSections, openBySection]);

  useEffect(() => {
    const callbacks = rows.map((row) => {
      if (row.kind === "collapse") {
        return () => {
          setOpenBySection((prev) => ({ ...prev, [row.section]: !prev[row.section] }));
        };
      }

      return () => {
        const item = menu?.[row.section].find((entry) => entry.value === row.value);
        if (!item) {
          return;
        }
        selectMenuItem(row.section, item);
      };
    });

    registerCallbacks(callbacks);

    return () => {
      registerCallbacks([]);
    };
  }, [rows, menu, selectMenuItem, registerCallbacks]);

  const isSidebarActive = focus.active && focus.panel === "sidebar";

  useEffect(() => {
    if (!isSidebarActive || focus.source === "mouse") {
      return;
    }

    const rafId = requestAnimationFrame(() => {
      const focusedItem = document.querySelector<HTMLElement>(
        `[data-sidebar-nav-index="${focus.sidebarIndex}"]`,
      );
      focusedItem?.scrollIntoView({ block: "nearest" });
    });

    return () => cancelAnimationFrame(rafId);
  }, [focus.sidebarIndex, focus.source, isSidebarActive, rows]);

  const showEmptyPlaceholder = !menu || visibleSections.length === 0;

  return (
    <SidebarContent className="overflow-hidden">
      <FolderActions
        focusedSidebarIndex={folderFocusedSidebarIndex}
        registerCallback={registerFolderCallback}
      />
      <div
        ref={setScrollContainer}
        className="no-scrollbar min-h-0 flex-1 overflow-auto overscroll-contain"
      >
        <SidebarGroup>
          <SidebarMenu>
            {showEmptyPlaceholder ? (
              <SidebarMenuItem className="px-1 py-2">
                <div className="space-y-1">
                  <SidebarMenuSkeleton showIcon />
                  <SidebarMenuSkeleton showIcon />
                  <SidebarMenuSkeleton showIcon />
                </div>
              </SidebarMenuItem>
            ) : (
              <SidebarNavProvider rows={rows} baseIndex={baseIndex}>
                {visibleSections.map((config) => (
                  <LibraryNavSection
                    key={config.section}
                    section={config.section}
                    label={config.label}
                    icon={config.icon}
                    items={config.visibleItems}
                    filter={filter}
                    open={openBySection[config.section]}
                    onToggleOpen={(open) => {
                      setOpenBySection((prev) => ({ ...prev, [config.section]: open }));
                    }}
                    onSelectItem={selectMenuItem}
                  />
                ))}
              </SidebarNavProvider>
            )}
          </SidebarMenu>
        </SidebarGroup>
      </div>
    </SidebarContent>
  );
};
