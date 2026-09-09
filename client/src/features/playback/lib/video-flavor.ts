export const FLAVORS = ['nature', 'underwater', 'space', 'city', 'countryside'] as const;

export type VideoFlavor = (typeof FLAVORS)[number];

export function isVideoFlavor(value: string): value is VideoFlavor {
  return FLAVORS.some((flavor) => flavor === value);
}

export function getNextFlavor(flavor: VideoFlavor): VideoFlavor {
  const index = FLAVORS.indexOf(flavor);

  return FLAVORS[(index + 1) % FLAVORS.length];
}
