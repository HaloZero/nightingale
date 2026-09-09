import { CheckCircle2Icon } from 'lucide-react';

import { GENERIC_DESCRIPTION, InfoLine, SoloFooter, type FocusCtx, type ViewParts } from '../parts';

type Args = {
  ctx: FocusCtx;
  version: string;
  onRestart: () => void;
};

export const installedView = ({ ctx, version, onRestart }: Args): ViewParts => ({
  description: GENERIC_DESCRIPTION,
  body: (
    <InfoLine icon={CheckCircle2Icon}>
      Update installed. Restart now to start using version {version}.
    </InfoLine>
  ),
  footer: <SoloFooter ctx={ctx} label="Restart now" onClick={onRestart} />,
});
