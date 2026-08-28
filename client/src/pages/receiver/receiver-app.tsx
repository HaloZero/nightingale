import { PlaybackProviders } from "@/contexts/playback";
import { loadSongsByHashes } from "@/bridge/songs";
import { useConfig } from "@/queries/use-config";
import { CAST_NAMESPACE } from "@/lib/cast/protocol";
import type { AppConfig } from "@/types/AppConfig";
import type { CastReceiverMessage } from "@/types/CastReceiverMessage";
import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { ReceiverLayout } from "./receiver-layout";

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
function useIncomingLoadMessage(): CastReceiverMessage | null {
  const [message, setMessage] = useState<CastReceiverMessage | null>(null);

  useEffect(() => {
    const params = new URLSearchParams(location.search);
    const fileHash = params.get("file_hash");
    if (fileHash) {
      const guideVolumeParam = params.get("guide_volume");
      setMessage({
        type: "load",
        file_hash: fileHash,
        guide_volume: guideVolumeParam ? Number(guideVolumeParam) : null,
      });
      return;
    }

    const context = window.cast?.framework?.CastReceiverContext.getInstance();
    if (!context) return;

    context.addCustomMessageListener<CastReceiverMessage>(CAST_NAMESPACE, (event) => {
      setMessage(event.data);
    });
    // Bypasses cast.framework's PlayerManager/MediaManager entirely (no
    // standard Media session -- playback is our own Web Audio graph via
    // useAudioPlayer), so the platform's inactivity auto-close has nothing
    // to key off; disable it explicitly or the receiver can get killed
    // mid-song.
    context.start({ disableIdleTimeout: true });
  }, []);

  return message;
}

function ReceiverContent() {
  const message = useIncomingLoadMessage();
  const { data: config } = useConfig();
  const fileHash = message?.file_hash;

  const { data: songs } = useQuery({
    queryKey: ["receiver-song", fileHash],
    queryFn: () => loadSongsByHashes([fileHash as string]),
    enabled: Boolean(fileHash),
  });
  const song = songs?.[0];

  // Mic-based pitch scoring is desktop-only (no mic access on a Chromecast
  // receiver, and it's out of scope here regardless) -- force it off rather
  // than pass `config` through unmodified, since `PlaybackMicProvider`
  // defaults `mic_active` to true when config is null.
  const effectiveConfig = useMemo<AppConfig | null>(() => {
    if (!config) return null;
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
      <ReceiverContent />
    </QueryClientProvider>
  );
}
