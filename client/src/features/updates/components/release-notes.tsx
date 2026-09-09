import Markdown, { type Components } from 'react-markdown';

import { openUrl } from '@/bridge/opener';
import { cn } from '@/shared/utils/cn';

const COMPONENTS: Components = {
  h3: ({ node: _node, className, children, ...props }) => (
    <h3
      className={cn(
        'text-[0.7rem] font-medium uppercase tracking-wide text-foreground first:mt-0 mt-3',
        className,
      )}
      {...props}
    >
      {children}
    </h3>
  ),
  p: ({ node: _node, className, ...props }) => (
    <p className={cn('text-xs first:mt-0 mt-2', className)} {...props} />
  ),
  ul: ({ node: _node, className, ...props }) => (
    <ul className={cn('list-disc pl-4 space-y-1 first:mt-0 mt-2', className)} {...props} />
  ),
  li: ({ node: _node, className, ...props }) => (
    <li className={cn('text-xs', className)} {...props} />
  ),
  a: ({ node: _node, href, children, ...props }) =>
    typeof href === 'string' && href !== '' ? (
      <a
        {...props}
        href={href}
        rel="noreferrer"
        onClick={(event) => {
          event.preventDefault();
          void openUrl(href);
        }}
        className="text-primary underline underline-offset-2 hover:text-primary/80"
      >
        {children}
      </a>
    ) : (
      <span {...props}>{children}</span>
    ),
  code: ({ node: _node, className, ...props }) => (
    <code
      className={cn('rounded bg-muted px-1 py-0.5 font-mono text-[0.7rem]', className)}
      {...props}
    />
  ),
};

type Props = {
  body: string;
  className?: string;
};

export const ReleaseNotes = ({ body, className }: Props) => (
  <div
    className={cn(
      'max-h-48 overflow-y-auto scrollbar-hide text-xs text-muted-foreground leading-relaxed break-words',
      className,
    )}
  >
    <Markdown components={COMPONENTS}>{body}</Markdown>
  </div>
);
