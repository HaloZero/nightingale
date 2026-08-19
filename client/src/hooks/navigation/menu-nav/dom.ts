export const SIDEBAR_SUB_SELECTOR = "[data-sidebar-sub-index]";
export const SIDEBAR_NAV_SELECTOR = "[data-sidebar-nav-index]";
export const ACTIONS_SELECTOR = "[data-actions-focus]";
export const SONG_SELECTOR = "[data-song-index]";

export type SongGridDirection = "up" | "down" | "left" | "right";

interface SongGridPosition {
  index: number;
  top: number;
  left: number;
  centerX: number;
}

export interface SidebarSubTarget {
  sidebarIndex: number;
  sidebarSubIndex: number;
}

function finiteDatasetNumber(value: string | undefined): number | null {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

export function blurActiveTextInput() {
  const active = document.activeElement;
  if (
    active instanceof HTMLElement &&
    (active.tagName === "INPUT" || active.tagName === "TEXTAREA" || active.isContentEditable)
  ) {
    active.blur();
  }
}

export function getHoveredSidebarSubTarget(target: Element | null): SidebarSubTarget | null {
  const subEl = target?.closest<HTMLElement>(SIDEBAR_SUB_SELECTOR);
  if (!subEl) {
    return null;
  }

  const parentEl = subEl.closest<HTMLElement>(SIDEBAR_NAV_SELECTOR);
  const sidebarIndex = finiteDatasetNumber(parentEl?.dataset.sidebarNavIndex);
  const sidebarSubIndex = finiteDatasetNumber(subEl.dataset.sidebarSubIndex);

  if (sidebarIndex === null || sidebarSubIndex === null) {
    return null;
  }

  return { sidebarIndex, sidebarSubIndex };
}

export function getHoveredSidebarIndex(target: Element | null): number | null {
  const sidebarEl = target?.closest<HTMLElement>(SIDEBAR_NAV_SELECTOR);
  return finiteDatasetNumber(sidebarEl?.dataset.sidebarNavIndex);
}

export function isActionsTarget(target: Element | null): boolean {
  return target?.closest(ACTIONS_SELECTOR) != null;
}

export function getHoveredSongIndex(target: Element | null): number | null {
  const songEl = target?.closest<HTMLElement>(SONG_SELECTOR);
  return finiteDatasetNumber(songEl?.dataset.songIndex);
}

export function isSongGrid(container: HTMLElement | null): container is HTMLElement {
  return container?.dataset.songLayout === "grid";
}

export function getSongGridTarget(
  container: HTMLElement,
  currentIndex: number,
  direction: SongGridDirection,
): number | null {
  const positions = Array.from(container.querySelectorAll<HTMLElement>(SONG_SELECTOR))
    .filter((element) => element.offsetWidth > 0 || element.offsetHeight > 0)
    .map((element) => {
      const index = finiteDatasetNumber(element.dataset.songIndex);
      if (index === null) return null;
      const rect = element.getBoundingClientRect();
      return {
        index,
        top: rect.top,
        left: rect.left,
        centerX: rect.left + rect.width / 2,
      };
    })
    .filter((position): position is SongGridPosition => position !== null);

  const current = positions.find((position) => position.index === currentIndex);
  if (!current) return null;

  const rowTops: number[] = [];
  positions
    .map((position) => position.top)
    .sort((a, b) => a - b)
    .forEach((top) => {
      if (!rowTops.some((rowTop) => Math.abs(rowTop - top) < 2)) rowTops.push(top);
    });
  const currentRow = rowTops.findIndex((top) => Math.abs(top - current.top) < 2);
  if (currentRow < 0) return null;

  if (direction === "left" || direction === "right") {
    const row = positions
      .filter((position) => Math.abs(position.top - current.top) < 2)
      .sort((a, b) => a.left - b.left);
    const column = row.findIndex((position) => position.index === currentIndex);
    const targetColumn = direction === "left" ? column - 1 : column + 1;
    return row[targetColumn]?.index ?? null;
  }

  const targetRowIndex = direction === "up" ? currentRow - 1 : currentRow + 1;
  const targetTop = rowTops[targetRowIndex];
  if (targetTop === undefined) return null;

  const targetRow = positions.filter((position) => Math.abs(position.top - targetTop) < 2);
  targetRow.sort(
    (a, b) => Math.abs(a.centerX - current.centerX) - Math.abs(b.centerX - current.centerX),
  );
  return targetRow[0]?.index ?? null;
}
