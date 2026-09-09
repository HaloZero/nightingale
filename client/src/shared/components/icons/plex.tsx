import type { SVGProps } from 'react';

/** Plex tile and chevron. Both inherit button text color. */
export const PlexIcon = (props: SVGProps<SVGSVGElement>) => (
  <svg
    viewBox="0 0 24 24"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
    focusable="false"
    {...props}
  >
    <rect
      x="0.75"
      y="0.75"
      width="22.5"
      height="22.5"
      rx="5"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
    />
    <path d="M6.9 3.2h5L17.1 12 12 20.8H6.9L12 12 6.9 3.2Z" fill="currentColor" />
  </svg>
);
