import { CheckCircle2Icon } from 'lucide-react';

import {
  CURRENT_VERSION,
  GENERIC_DESCRIPTION,
  InfoLine,
  PairFooter,
  type FocusCtx,
  type ViewParts,
} from '../parts';

type Args = {
  ctx: FocusCtx;
  onClose: () => void;
  onRefetch: () => void;
};

export const upToDateView = ({ ctx, onClose, onRefetch }: Args): ViewParts => ({
  description: GENERIC_DESCRIPTION,
  body: (
    <InfoLine icon={CheckCircle2Icon}>You're on the latest version (v{CURRENT_VERSION}).</InfoLine>
  ),
  footer: (
    <PairFooter ctx={ctx} onClose={onClose} primaryLabel="Check again" onPrimary={onRefetch} />
  ),
});
