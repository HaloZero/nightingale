import { GENERIC_DESCRIPTION, PairFooter, type FocusCtx, type ViewParts } from '../parts';

type Args = {
  ctx: FocusCtx;
  message: string;
  onClose: () => void;
  onRetry: () => void;
};

export const installErrorView = ({ ctx, message, onClose, onRetry }: Args): ViewParts => ({
  description: GENERIC_DESCRIPTION,
  body: (
    <div className="flex flex-col gap-2 text-xs">
      <p className="text-destructive">The update could not be installed.</p>
      <p className="text-muted-foreground break-words">{message}</p>
    </div>
  ),
  footer: (
    <PairFooter ctx={ctx} onClose={onClose} primaryLabel="Retry install" onPrimary={onRetry} />
  ),
});
