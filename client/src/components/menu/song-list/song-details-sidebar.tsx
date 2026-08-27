import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { Separator } from "@/components/ui/separator";
import { useBestScoresBySongForActiveProfile } from "@/hooks/use-best-scores-by-song";
import { usePreparePlaybackMutation } from "@/mutations/use-prepare-playback-mutation";
import { SONGS } from "@/queries/keys";
import type { QueuedStatus } from "@/types/QueuedStatus";
import type { Song } from "@/types/Song";
import { useQueryClient } from "@tanstack/react-query";
import { PlayIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { ActionsSection } from "./details/actions-section";
import { KeyTempoSection } from "./details/key-tempo-section";
import { SongDetailsHeader } from "./details/song-details-header";
import { useSongDetailsNav } from "./details/use-song-details-nav";
import { getSongStatusInfo } from "./shared/song-status";

interface SongDetailsSidebarProps {
  song: Song;
  queueStatus?: QueuedStatus;
  onClose: () => void;
}

export const SongDetailsSidebar = ({ song, queueStatus, onClose }: SongDetailsSidebarProps) => {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const bestScores = useBestScoresBySongForActiveProfile();
  const { detailsRef, closeDetails } = useSongDetailsNav(onClose);
  const { mutate: preparePlayback, isLoading: preparingPlayback } = usePreparePlaybackMutation();
  const [tempo, setTempo] = useState(song.tempo);
  const [keyOffset, setKeyOffset] = useState(song.key_offset);

  const status = getSongStatusInfo(song.is_analyzed, queueStatus);
  const analysisBusy = queueStatus === "Queued" || Boolean(status.isAnalyzing);
  // LRC songs played over the original mix are playable immediately while their
  // key is still being detected off-queue. Until the key lands, treat the
  // key/tempo section as pending rather than showing controls.
  const keyPending =
    song.is_analyzed && song.transcript_source === "Lrc" && song.no_stems && song.key === null;
  const supportsShifts = song.is_analyzed && song.transcript_source !== "Usdx" && !keyPending;
  const supportsAnalysisActions = Boolean(status.isReady) && song.transcript_source !== "Usdx";

  // supportsAnalysisActions gates the *entire* analysis-actions group in
  // ActionsSection, including karaoke video rendering -- and status.isReady
  // depends on queueStatus, not just song.is_analyzed (see
  // getSongStatusInfo: a "Queued"/Analyzing/Failed queue row makes isReady
  // false even for an already-analyzed song, e.g. a stale/lingering queue
  // entry, or a song re-queued for realign/reanalysis). If the actions menu
  // for an analyzed song looks emptier than expected, check this line
  // first instead of guessing -- it names exactly which of the three
  // conditions is the blocker.
  console.debug("[song-details-sidebar] action gating", {
    file_hash: song.file_hash,
    title: song.title,
    is_analyzed: song.is_analyzed,
    transcript_source: song.transcript_source,
    queueStatus,
    status,
    supportsAnalysisActions,
  });

  // Off-queue key detection doesn't invalidate any query, so poll the song list
  // while the key is pending to pick it up and unlock the shift controls.
  useEffect(() => {
    if (!keyPending) return;
    const interval = setInterval(() => {
      queryClient.invalidateQueries({ queryKey: SONGS });
    }, 2000);
    return () => clearInterval(interval);
  }, [keyPending, queryClient]);

  const handlePlay = () => {
    const hasAdjustments = keyOffset !== song.key_offset || tempo !== song.tempo;

    if (!hasAdjustments) {
      navigate("/playback", { state: { song } });
      return;
    }

    preparePlayback(
      { song, tempo, keyOffset },
      {
        onSuccess: (preparedSong) => navigate("/playback", { state: { song: preparedSong } }),
      },
    );
  };

  return (
    <aside
      ref={detailsRef}
      className="flex min-h-0 min-w-0 flex-1 flex-col border-l bg-background [&_[data-song-details-focused=true]]:z-10 [&_[data-song-details-focused=true]]:ring-2 [&_[data-song-details-focused=true]]:ring-primary xl:w-96 xl:flex-none"
      aria-label="Song details"
    >
      <SongDetailsHeader
        song={song}
        queueStatus={queueStatus}
        bestScore={bestScores.get(song.file_hash)}
        onClose={closeDetails}
      />

      <div className="no-scrollbar min-h-0 flex-1 overflow-y-auto">
        <KeyTempoSection
          song={song}
          supportsShifts={supportsShifts}
          tempo={tempo}
          keyOffset={keyOffset}
          disabled={preparingPlayback}
          onTempoChange={setTempo}
          onKeyOffsetChange={setKeyOffset}
        />

        <Separator />

        <ActionsSection
          song={song}
          status={status}
          analysisBusy={analysisBusy}
          supportsAnalysisActions={supportsAnalysisActions}
        />
      </div>

      <footer className="border-t p-3 pb-[max(0.75rem,env(safe-area-inset-bottom))]">
        <Button
          size="lg"
          className="h-8 w-full disabled:bg-primary/50 disabled:text-primary-foreground/45 disabled:opacity-100"
          disabled={!status.isReady || preparingPlayback}
          aria-busy={preparingPlayback}
          onClick={handlePlay}
        >
          {preparingPlayback ? (
            <>
              <Spinner className="size-4" /> Preparing playback…
            </>
          ) : (
            <>
              <PlayIcon /> Play
            </>
          )}
        </Button>
      </footer>
    </aside>
  );
};
