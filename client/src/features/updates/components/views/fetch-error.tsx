import { WifiOffIcon } from 'lucide-react';

import { GENERIC_DESCRIPTION, PairFooter, type FocusCtx, type ViewParts } from '../parts';

type Args = {
  ctx: FocusCtx;
  error: Error;
  isOffline: boolean;
  onClose: () => void;
  onRetry: () => void;
};

export const fetchErrorView = ({ ctx, error, isOffline, onClose, onRetry }: Args): ViewParts => {
  const headline = isOffline ? "Couldn't reach the update server." : "Couldn't check for updates.";
  const hint = isOffline ? 'Check your internet connection and try again.' : error.message;

  return {
    description: GENERIC_DESCRIPTION,
    body: (
      <div className="flex items-start gap-2 text-xs">
        {isOffline && <WifiOffIcon className="size-4 text-destructive shrink-0 mt-0.5" />}
        <div className="flex flex-col gap-1">
          <p className="text-destructive">{headline}</p>
          <p className="text-muted-foreground break-words">{hint}</p>
        </div>
      </div>
    ),
    footer: <PairFooter ctx={ctx} onClose={onClose} primaryLabel="Retry" onPrimary={onRetry} />,
  };
};
