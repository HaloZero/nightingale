import { ExternalLinkIcon, InfoIcon, ServerIcon } from 'lucide-react';

import type { UnsupportedChannel } from '@/features/updates/queries/use-update';

import { CURRENT_VERSION, InfoLine, PairFooter, type FocusCtx, type ViewParts } from '../parts';

type Args = {
  ctx: FocusCtx;
  channel: UnsupportedChannel;
  onClose: () => void;
  onOpenReleases: () => void;
  onOpenSelfHostedDocs: () => void;
};

/**
 * Two distinct "manual update" stories share the same dialog layout:
 *
 *  - Tauri Linux build: no bundler-supported auto-update, user downloads
 *    the next release tarball from GitHub.
 *  - Self-hosted web build: nightingale runs as a service on a host the
 *    user controls; updating means SSHing in and re-running the install
 *    script (which already preserves data / config / library).
 *
 * The desktop-Linux copy is byte-identical to the previous version of this
 * view so behaviour for that build doesn't regress.
 */
export const unsupportedView = ({
  ctx,
  channel,
  onClose,
  onOpenReleases,
  onOpenSelfHostedDocs,
}: Args): ViewParts => {
  if (channel === 'self-hosted-web') {
    return selfHostedWebView({ ctx, onClose, onOpenSelfHostedDocs });
  }
  return linuxTauriView({ ctx, onClose, onOpenReleases });
};

type LinuxArgs = {
  ctx: FocusCtx;
  onClose: () => void;
  onOpenReleases: () => void;
};

const linuxTauriView = ({ ctx, onClose, onOpenReleases }: LinuxArgs): ViewParts => ({
  description: 'Linux builds use manual updates from GitHub Releases.',
  body: (
    <InfoLine icon={InfoIcon}>
      Auto-update isn't supported on Linux. Download the latest release from GitHub to update
      Nightingale (you're on v{CURRENT_VERSION}).
    </InfoLine>
  ),
  footer: (
    <PairFooter
      ctx={ctx}
      onClose={onClose}
      primaryLabel="Open GitHub Releases"
      primaryIcon={ExternalLinkIcon}
      onPrimary={onOpenReleases}
    />
  ),
});

type SelfHostedArgs = {
  ctx: FocusCtx;
  onClose: () => void;
  onOpenSelfHostedDocs: () => void;
};

const selfHostedWebView = ({ ctx, onClose, onOpenSelfHostedDocs }: SelfHostedArgs): ViewParts => ({
  description: 'Self-hosted instance — update by re-running the install script on the host.',
  body: (
    <div className="flex flex-col gap-3">
      <InfoLine icon={ServerIcon}>
        You're using a self-hosted Nightingale instance (v{CURRENT_VERSION}) running on a Linux box
        on your LAN. The browser tab can't update the server — SSH into the host and re-run the
        installer. Your data folder, library database, and config are preserved across upgrades.
      </InfoLine>
      <pre className="rounded-md bg-muted px-3 py-2 text-[0.7rem] font-mono leading-relaxed whitespace-pre-wrap break-all">
        curl -fsSL https://raw.githubusercontent.com/rzru/nightingale/main/scripts/install.sh | bash
      </pre>
    </div>
  ),
  footer: (
    <PairFooter
      ctx={ctx}
      onClose={onClose}
      primaryLabel="Open self-hosted docs"
      primaryIcon={ExternalLinkIcon}
      onPrimary={onOpenSelfHostedDocs}
    />
  ),
});
