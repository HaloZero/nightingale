import type { SVGProps } from 'react';

/**
 * Outlined Jellyfin monogram (the recognisable two-stack triangle). Strokes
 * inherit the surrounding tone via `currentColor` so the icon themes itself
 * just like the lucide ones, and the parent button's `[&_svg]:size-*`
 * tailwind selectors handle sizing.
 */
export const JellyfinIcon = (props: SVGProps<SVGSVGElement>) => (
  <svg
    viewBox="0 0 48 48"
    xmlns="http://www.w3.org/2000/svg"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
    focusable="false"
    {...props}
  >
    <path d="M24,20c-1.62,0-6.85,9.48-6.06,11.08s11.33,1.59,12.12,0S25.63,20,24,20Z" />
    <path d="M24,5.5C19.11,5.5,3.34,34.08,5.75,38.9s34.13,4.77,36.51,0S28.9,5.5,24,5.5ZM36,34.71c-1.56,3.13-22.35,3.17-23.93,0S20.8,12.83,24,12.83,37.52,31.59,36,34.71Z" />
  </svg>
);
