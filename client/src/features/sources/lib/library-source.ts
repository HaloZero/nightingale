import type { AppConfig } from '@/types/AppConfig';
import type { LibrarySource } from '@/types/LibrarySource';

const isKind = <K extends LibrarySource['kind']>(
  src: LibrarySource | null | undefined,
  kind: K,
): src is Extract<LibrarySource, { kind: K }> => src?.kind === kind;

/**
 * Narrow `AppConfig.library_source` to a specific variant. One place owns
 * the discriminant check so call sites don't open-code
 * `config?.library_source?.kind === "..." ? ... : null` and so adding a new
 * source kind (Navidrome, Subsonic) is automatically covered by TS.
 */
export const getSource = <K extends LibrarySource['kind']>(
  config: AppConfig | null | undefined,
  kind: K,
): Extract<LibrarySource, { kind: K }> | null => {
  const src = config?.library_source;

  return isKind(src, kind) ? src : null;
};
