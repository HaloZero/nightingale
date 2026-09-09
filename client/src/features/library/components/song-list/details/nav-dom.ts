import { DIALOG_FOCUSABLE_SELECTOR } from '@/features/menu/hooks/use-dialog-nav';

const NAV_GROUP_SELECTOR = '[data-song-details-nav-group]';

export type NavigationRow = {
  size: number;
  horizontal: boolean;
};

export function getDetailsFocusables(container: HTMLElement | null): HTMLElement[] {
  if (!container) {
    return [];
  }
  return Array.from(container.querySelectorAll<HTMLElement>(DIALOG_FOCUSABLE_SELECTOR)).filter(
    (element) => element.offsetWidth > 0 || element.offsetHeight > 0,
  );
}

export function getNavigationRows(focusables: HTMLElement[]): NavigationRow[] {
  const rows: NavigationRow[] = [];
  let previousGroup: Element | null = null;

  focusables.forEach((element) => {
    const group = element.closest(NAV_GROUP_SELECTOR);
    if (group && group === previousGroup) {
      rows[rows.length - 1].size += 1;
    } else {
      rows.push({ size: 1, horizontal: group !== null });
    }
    previousGroup = group;
  });

  return rows.length > 0 ? rows : [{ size: 1, horizontal: false }];
}
