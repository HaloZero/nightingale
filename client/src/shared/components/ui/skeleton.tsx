import { cn } from '@/shared/utils/cn';

function Skeleton({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="skeleton"
      className={cn('animate-pulse rounded-md bg-foreground/10 dark:bg-foreground/15', className)}
      {...props}
    />
  );
}

export { Skeleton };
