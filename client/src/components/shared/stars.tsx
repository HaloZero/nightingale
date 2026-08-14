import { cn } from "@/lib/utils";
import { halfStarUnits } from "@/utils/playback/result";
import { StarHalfIcon, StarIcon } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

type Size = "sm" | "lg";

const sizeClass: Record<Size, string> = {
  sm: "size-2.5",
  lg: "size-6",
};

interface Props {
  score: number;
  size?: Size;
  className?: string;
}

export const Stars = ({ score, size = "lg", className }: Props) => {
  const hs = halfStarUnits(score);
  const filled = Math.floor(hs / 2);
  const hasHalf = hs % 2 === 1;
  const empty = 5 - filled - (hasHalf ? 1 : 0);
  const ic = sizeClass[size];

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          className={cn("flex w-fit flex-row items-center gap-1", className)}
          role="img"
          aria-label={`Score ${score}`}
        >
          {Array.from({ length: filled }, (_, i) => (
            <StarIcon key={`f-${i}`} className={cn(ic, "fill-primary text-primary")} />
          ))}
          {hasHalf ? <StarHalfIcon className={cn(ic, "fill-primary text-primary")} /> : null}
          {Array.from({ length: empty }, (_, i) => (
            <StarIcon key={`e-${i}`} className={cn(ic, "text-muted-foreground/25")} />
          ))}
        </div>
      </TooltipTrigger>
      <TooltipContent side="right" sideOffset={4}>
        Score: {score}
      </TooltipContent>
    </Tooltip>
  );
};
