export const SIDEBAR_SUB_SELECTOR = '[data-sidebar-sub-index]';
export const SIDEBAR_NAV_SELECTOR = '[data-sidebar-nav-index]';
export const ACTIONS_SELECTOR = '[data-actions-index]';
export const SONG_SELECTOR = '[data-song-index]';

export type SongGridDirection = 'up' | 'down' | 'left' | 'right';

type SongGridPosition = {
  index: number;
  top: number;
  left: number;
  centerX: number;
};

export type SidebarSubTarget = {
  sidebarIndex: number;
  sidebarSubIndex: number;
};

function finiteDatasetNumber(value: string | undefined): number | null {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

export function blurActiveTextInput() {
  const active = document.activeElement;
  if (
    active instanceof HTMLElement &&
    (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.isContentEditable)
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

export function getActionsCount(): number {
  return document.querySelectorAll(ACTIONS_SELECTOR).length;
}

export function getActionTarget(index: number): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-actions-index="${index}"]`);
}

export function getHoveredActionsIndex(target: Element | null): number | null {
  const actionsEl = target?.closest<HTMLElement>(ACTIONS_SELECTOR);
  return finiteDatasetNumber(actionsEl?.dataset.actionsIndex);
}

export function getHoveredSongIndex(target: Element | null): number | null {
  const songEl = target?.closest<HTMLElement>(SONG_SELECTOR);
  return finiteDatasetNumber(songEl?.dataset.songIndex);
}

export function isSongGrid(container: HTMLElement | null): container is HTMLElement {
  return container?.dataset.songLayout === 'grid';
}

function songGridPositions(container: HTMLElement): SongGridPosition[] {
  return Array.from(container.querySelectorAll<HTMLElement>(SONG_SELECTOR))
    .filter((element) => element.offsetWidth > 0 || element.offsetHeight > 0)
    .map((element) => {
      const index = finiteDatasetNumber(element.dataset.songIndex);
      if (index === null) {
        return null;
      }
      const rect = element.getBoundingClientRect();
      return {
        index,
        top: rect.top,
        left: rect.left,
        centerX: rect.left + rect.width / 2,
      };
    })
    .filter((position): position is SongGridPosition => position !== null);
}

function uniqueRowTops(positions: readonly SongGridPosition[]): number[] {
  const rowTops: number[] = [];
  for (const top of positions.map((position) => position.top).toSorted((a, b) => a - b)) {
    if (!rowTops.some((rowTop) => Math.abs(rowTop - top) < 2)) {
      rowTops.push(top);
    }
  }
  return rowTops;
}

function horizontalGridTarget(
  positions: readonly SongGridPosition[],
  current: SongGridPosition,
  currentIndex: number,
  direction: 'left' | 'right',
): number | null {
  const row = positions
    .filter((position) => Math.abs(position.top - current.top) < 2)
    .toSorted((a, b) => a.left - b.left);
  const column = row.findIndex((position) => position.index === currentIndex);
  const targetColumn = direction === 'left' ? column - 1 : column + 1;
  return row[targetColumn]?.index ?? null;
}

function verticalGridTarget(
  positions: readonly SongGridPosition[],
  current: SongGridPosition,
  direction: 'up' | 'down',
): number | null {
  const rowTops = uniqueRowTops(positions);
  const currentRow = rowTops.findIndex((top) => Math.abs(top - current.top) < 2);
  if (currentRow < 0) {
    return null;
  }
  const targetRowIndex = direction === 'up' ? currentRow - 1 : currentRow + 1;
  if (targetRowIndex < 0 || targetRowIndex >= rowTops.length) {
    return null;
  }
  const targetTop = rowTops[targetRowIndex];

  const targetRow = positions
    .filter((position) => Math.abs(position.top - targetTop) < 2)
    .toSorted(
      (a, b) => Math.abs(a.centerX - current.centerX) - Math.abs(b.centerX - current.centerX),
    );
  return targetRow.at(0)?.index ?? null;
}

export function getSongGridTarget(
  container: HTMLElement,
  currentIndex: number,
  direction: SongGridDirection,
): number | null {
  const positions = songGridPositions(container);
  const current = positions.find((position) => position.index === currentIndex);
  if (!current) {
    return null;
  }

  return direction === 'left' || direction === 'right'
    ? horizontalGridTarget(positions, current, currentIndex, direction)
    : verticalGridTarget(positions, current, direction);
}
