import { LoaderCircleIcon } from "lucide-react";

import { useVideoQueue } from "@/queries/use-songs";

function label(youtubeCount: number, reelCount: number): string {
  const parts: string[] = [];
  if (youtubeCount > 0) parts.push(`${youtubeCount} YouTube video${youtubeCount === 1 ? "" : "s"}`);
  if (reelCount > 0) parts.push(`${reelCount} karaoke render${reelCount === 1 ? "" : "s"}`);
  return `Processing ${parts.join(", ")}...`;
}

export const VideoQueueIndicator = () => {
  const { data } = useVideoQueue();

  if (!data || data.entries.length === 0) {
    return null;
  }

  const youtubeCount = data.entries.filter((e) => e.kind === "Youtube").length;
  const reelCount = data.entries.filter((e) => e.kind === "Reel").length;

  return (
    <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
      <LoaderCircleIcon className="size-3 animate-spin text-green-500" />
      {label(youtubeCount, reelCount)}
    </div>
  );
};
