import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';
import { MemoryRouter } from 'react-router';

import { loadSongsByHashes } from '@/bridge/songs';
import { CAST_NAMESPACE } from '@/features/playback/lib/cast/protocol';
import { PlaybackProviders } from '@/features/playback/providers';
import { useConfig } from '@/shared/config/use-config';
import type { AppConfig } from '@/types/AppConfig';
import type { CastReceiverMessage } from '@/types/CastReceiverMessage';

import { ReceiverLayout } from './receiver-layout';

const queryClient = new QueryClient();

/**
 * Registers the CAF custom-message listener and returns the most recent
 * `Load` message. `?file_hash=...` in the query string always wins over a
 * real Cast session, checked first -- the gstatic CAF SDK script tag in
 * receiver.html still loads and defines `window.cast.framework` in a plain
 * desktop browser tab (it has no way to know it isn't actually running on a
 * Chromecast), so branching on "does `window.cast.framework` exist" is not
 * a reliable signal for "are we in a real Cast session." A real Cast launch
 * never carries `file_hash` on the receiver URL, so there's no ambiguity in
 * practice -- this lets the whole render path be exercised from a plain
 * browser tab without a physical Chromecast, see the plan doc's
 * verification section.
 */
function urlLoadMessage(): CastReceiverMessage | null {
  const params = new URLSearchParams(location.search);
  const fileHash = params.get('file_hash');
  if (fileHash === null) {
    return null;
  }
  const guideVolumeParam = params.get('guide_volume');
  return {
    type: 'load',
    file_hash: fileHash,
    guide_volume: guideVolumeParam !== null ? Number(guideVolumeParam) : null,
  };
}

function useIncomingLoadMessage(): CastReceiverMessage | null {
  // `?file_hash=...` in the query string always wins over a real Cast
  // session -- the gstatic CAF SDK script tag in receiver.html still loads
  // and defines `window.cast.framework` in a plain desktop browser tab (it
  // has no way to know it isn't actually running on a Chromecast), so
  // branching on "does `window.cast.framework` exist" is not a reliable
  // signal for "are we in a real Cast session." A real Cast launch never
  // carries `file_hash` on the receiver URL, so there's no ambiguity in
  // practice -- this lets the whole render path be exercised from a plain
  // browser tab without a physical Chromecast, see the plan doc's
  // verification section. Read once via lazy `useState` init (the query
  // string doesn't change without a full page reload) rather than an
  // effect, so this branch never costs an extra render.
  const [urlMessage] = useState<CastReceiverMessage | null>(urlLoadMessage);
  const [castMessage, setCastMessage] = useState<CastReceiverMessage | null>(null);

  useEffect(() => {
    if (urlMessage !== null) {
      return;
    }

    const framework = window.cast?.framework;
    if (framework === undefined) {
      return;
    }
    const context = framework.CastReceiverContext.getInstance();

    context.addCustomMessageListener<CastReceiverMessage>(CAST_NAMESPACE, (event) => {
      setCastMessage(event.data);
    });
    // Bypasses cast.framework's PlayerManager/MediaManager entirely (no
    // standard Media session -- playback is our own Web Audio graph via
    // useAudioPlayer), so the platform's inactivity auto-close has nothing
    // to key off; disable it explicitly or the receiver can get killed
    // mid-song.
    context.start({ disableIdleTimeout: true });
  }, [urlMessage]);

  return urlMessage ?? castMessage;
}

function ReceiverContent() {
  const message = useIncomingLoadMessage();
  const { data: config } = useConfig();
  const fileHash = message?.file_hash;

  const { data: songs } = useQuery({
    queryKey: ['receiver-song', fileHash],
    queryFn: () => {
      if (fileHash === undefined) {
        throw new Error('fileHash is required');
      }
      return loadSongsByHashes([fileHash]);
    },
    enabled: fileHash !== undefined,
  });
  const song = songs?.[0];

  // Mic-based pitch scoring is desktop-only (no mic access on a Chromecast
  // receiver, and it's out of scope here regardless) -- force it off rather
  // than pass `config` through unmodified, since `PlaybackMicProvider`
  // defaults `mic_active` to true when config is null.
  const effectiveConfig = useMemo<AppConfig | null>(() => {
    if (!config) {
      return null;
    }
    return {
      ...config,
      guide_volume: message?.guide_volume ?? config.guide_volume,
      mic_active: false,
      mic_monitoring: false,
    };
  }, [config, message?.guide_volume]);

  if (!song || !effectiveConfig) {
    return null;
  }

  return (
    <PlaybackProviders song={song} config={effectiveConfig}>
      <ReceiverLayout config={effectiveConfig} />
    </PlaybackProviders>
  );
}

export function ReceiverApp() {
  return (
    <QueryClientProvider client={queryClient}>
      {/* PlaybackTransportProvider (reused as-is from PlaybackProviders)
          calls useNavigate() for its exit/error-recovery paths -- a
          desktop-only "back to the menu" action with no receiver
          equivalent. MemoryRouter satisfies that hook without touching
          the real address bar; since nothing renders a <Routes> off it,
          any navigate("/") call is simply inert here. */}
      <MemoryRouter>
        <ReceiverContent />
      </MemoryRouter>
    </QueryClientProvider>
  );
}
