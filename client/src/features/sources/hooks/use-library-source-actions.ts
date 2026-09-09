import { getServerFlags } from '@/bridge/server-flags';
import { getSource } from '@/features/sources/lib/library-source';
import {
  useDisconnectSource,
  useRescan,
  useSelectFolderSource,
} from '@/features/sources/mutations/use-source-mutations';
import { useJellyfinHealth } from '@/features/sources/queries/use-jellyfin-health';
import { useNavidromeHealth } from '@/features/sources/queries/use-navidrome-health';
import { usePlexHealth } from '@/features/sources/queries/use-plex-health';
import { useConfig } from '@/shared/config/use-config';
import type { AppConfig } from '@/types/AppConfig';
import type { LibrarySource } from '@/types/LibrarySource';

const sourceUnavailable = (
  kind: string | undefined,
  jellyfinReachable: boolean | undefined,
  navidromeReachable: boolean | undefined,
  plexReachable: boolean | undefined,
): boolean => {
  if (kind === 'jellyfin') {
    return jellyfinReachable === false;
  }
  if (kind === 'navidrome') {
    return navidromeReachable === false;
  }
  if (kind === 'plex') {
    return plexReachable === false;
  }

  return false;
};

const configuredSource = (config: AppConfig | undefined): LibrarySource | null =>
  config?.library_source ?? null;

const sourceKind = (source: LibrarySource | null): LibrarySource['kind'] | undefined =>
  source === null ? undefined : source.kind;

const sourceIsKind = (source: LibrarySource | null, kind: LibrarySource['kind']): boolean =>
  source?.kind === kind;

/**
 * Coordinated source actions used by the sidebar + empty-state. Wraps the
 * `bridge/source` mutations and exposes precomputed booleans for "what's the
 * active source" / "is rescan enabled".
 */
export const useLibrarySourceActions = () => {
  const { data: config } = useConfig();
  const { data: jellyfinHealth } = useJellyfinHealth();
  const { data: navidromeHealth } = useNavidromeHealth();
  const { data: plexHealth } = usePlexHealth();

  const folderMutation = useSelectFolderSource();
  const rescanMutation = useRescan();
  const disconnectMutation = useDisconnectSource();

  // When the operator pins the library folder (NIGHTINGALE_LIBRARY_PATH), the
  // source is configured server-side and picking one in-app is disabled.
  const { libraryPinned } = getServerFlags();

  const source = configuredSource(config);
  const hasSource = source !== null;
  const jellyfinSource = getSource(config, 'jellyfin');
  const navidromeSource = getSource(config, 'navidrome');
  const plexSource = getSource(config, 'plex');
  const isFolderSource = sourceIsKind(source, 'folder');
  const isJellyfinSource = jellyfinSource !== null;
  const isNavidromeSource = navidromeSource !== null;
  const isPlexSource = plexSource !== null;

  const isPending = [
    folderMutation.isPending,
    rescanMutation.isPending,
    disconnectMutation.isPending,
  ].some(Boolean);

  // Rescan is safe to fire whenever there's an active source — start_scan()
  // bumps the cancellation generation, so kicking off a new one while one is
  // already running just supersedes the old. The only hard-blocks are: no
  // source at all, or a remote source whose server we know is offline.
  const rescanDisabled =
    !hasSource ||
    rescanMutation.isPending ||
    sourceUnavailable(
      sourceKind(source),
      jellyfinHealth?.reachable,
      navidromeHealth?.reachable,
      plexHealth?.reachable,
    );

  return {
    config,
    jellyfinHealth,
    navidromeHealth,
    plexHealth,
    hasSource,
    libraryPinned,
    jellyfinSource,
    navidromeSource,
    plexSource,
    isFolderSource,
    isJellyfinSource,
    isNavidromeSource,
    isPlexSource,
    selectFolder: () => folderMutation.mutate(),
    rescan: () => rescanMutation.mutate(),
    disconnectSource: () => disconnectMutation.mutate(),
    isPending,
    rescanDisabled,
  };
};
