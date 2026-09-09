import { cn } from '@/shared/utils/cn';

export const RING_CLASS = 'ring-2 ring-primary';
export const NO_FOCUS_RING_CLASS = 'focus-visible:ring-0 focus-visible:border-transparent';

// We keep visually-disabled buttons in the DOM (without the `disabled` attribute)
// so that `useDialogNav` doesn't drop them from its focusables list and shift
// every later button's slot. Click handlers must guard against the boundary
// case themselves.
export const ARIA_DISABLED_CLASS = 'aria-disabled:opacity-50 aria-disabled:cursor-not-allowed';

export const ringFor = (focused: boolean): string => cn(NO_FOCUS_RING_CLASS, focused && RING_CLASS);
