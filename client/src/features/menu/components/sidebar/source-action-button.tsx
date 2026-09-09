import { type ComponentType, type SVGProps } from 'react';

import { Button } from '@/shared/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/shared/components/ui/tooltip';
import { cn } from '@/shared/utils/cn';

export type BadgeTone = 'ok' | 'warn' | 'muted';

const BADGE_CLASSES: Record<BadgeTone, string> = {
  ok: 'bg-chart-3',
  warn: 'bg-destructive',
  muted: 'bg-muted-foreground',
};

export type SourceActionButtonProps = {
  icon: ComponentType<SVGProps<SVGSVGElement>>;
  label: string;
  tooltip: string;
  disabled?: boolean;
  focused?: boolean;
  badge?: BadgeTone;
  onClick: () => void;
  /** Sidebar focus index used by the menu keyboard nav system. */
  subIndex: number;
};

/**
 * One icon button in the Library cluster. Renders the button itself plus an
 * optional status dot — used today to surface Jellyfin reachability without
 * stealing space from the rest of the sidebar.
 */
export const SourceActionButton = ({
  icon: Icon,
  label,
  tooltip,
  disabled,
  focused,
  badge,
  onClick,
  subIndex,
}: SourceActionButtonProps) => (
  <Tooltip>
    <TooltipTrigger asChild>
      <span className="relative inline-flex">
        <Button
          tabIndex={-1}
          variant="ghost"
          size="icon-xs"
          aria-label={label}
          disabled={disabled}
          onClick={onClick}
          data-sidebar-sub-index={subIndex}
          className={cn(
            'text-sidebar-foreground/70 hover:bg-transparent hover:text-sidebar-foreground/70 focus-visible:ring-0 focus-visible:border-transparent dark:hover:bg-transparent',
            focused === true && 'ring-2 ring-primary bg-sidebar-accent',
          )}
        >
          <Icon />
        </Button>
        {badge && (
          <span
            aria-hidden
            className={cn(
              'pointer-events-none absolute -right-0.5 -top-0.5 size-1.5 rounded-full ring-1 ring-sidebar',
              BADGE_CLASSES[badge],
            )}
          />
        )}
      </span>
    </TooltipTrigger>
    <TooltipContent>{tooltip}</TooltipContent>
  </Tooltip>
);
