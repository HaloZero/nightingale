/**
 * Chromecast receiver's presentational tree -- deliberately smaller than
 * `client/src/pages/playback/playback-inner.tsx`'s `PlaybackLayout`: no
 * `PlaybackHud`, `PitchGraph`, `PauseOverlay`, `ResultDialog`, or
 * `usePlaybackInput` (all mic-scoring/keyboard-shortcut/local-remote-control
 * concerns that don't apply on a TV with no keyboard or mic). Just the
 * background and synced lyrics, which is exactly what was asked for.
 */

import './receiver.css';

import { Background } from '@/features/playback/components/background';
import { LyricsDisplay } from '@/features/playback/components/lyrics-display';
import { usePlaybackTranscriptState } from '@/features/playback/providers/playback-transcript-context';
import { usePlaybackTransportState } from '@/features/playback/providers/playback-transport-context';
import type { AppConfig } from '@/types/AppConfig';

type ReceiverLayoutProps = {
  config: AppConfig | null;
};

export function ReceiverLayout({ config }: ReceiverLayoutProps) {
  const { isReady } = usePlaybackTransportState();
  const { segments } = usePlaybackTranscriptState();

  const verticalPosition = config?.lyrics_vertical_position ?? 'bottom';
  const horizontalPosition = config?.lyrics_horizontal_position ?? 'center';

  return (
    <div className="fixed inset-0 overflow-hidden bg-black" style={{ contain: 'strict' }}>
      {/* Background shows a shader loading-animation while !isReady (see
          background.tsx) -- desirable on desktop, but a plain black screen
          reads better on a TV during the brief load. Not mounting it at all
          until ready lets the wrapper's own bg-black show through instead,
          without touching Background's shared desktop behavior. */}
      {isReady && <Background />}

      {isReady && (
        <LyricsDisplay
          segments={segments}
          verticalPosition={verticalPosition}
          horizontalPosition={horizontalPosition}
          classNames={{
            currentPill: 'receiver-lyrics-pill',
            currentLine: 'receiver-lyrics-line',
            nextPill: 'receiver-lyrics-next-pill',
            nextLine: 'receiver-lyrics-next-line',
          }}
        />
      )}
    </div>
  );
}
