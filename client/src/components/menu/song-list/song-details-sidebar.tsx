import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import type { QueuedStatus } from "@/types/QueuedStatus";
import type { Song } from "@/types/Song";
import { SONGS } from "@/queries/keys";
import { useQueryClient } from "@tanstack/react-query";
import { PlayIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { ActionsSection } from "./details/actions-section";
import { KeyTempoSection } from "./details/key-tempo-section";
import { SongDetailsHeader } from "./details/song-details-header";
import { useSongDetailsNav } from "./details/use-song-details-nav";
import { getSongStatusInfo } from "./shared/song-status";
import type { ShiftType } from "./shifts";
import { useBestScoresBySongForActiveProfile } from "@/hooks/use-best-scores-by-song";

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
  const [shifting, setShifting] = useState<Record<ShiftType, boolean>>({
    tempo: false,
    key: false,
  });

  const status = getSongStatusInfo(song.is_analyzed, queueStatus);
  const analysisBusy = queueStatus === "Queued" || Boolean(status.isAnalyzing);
  // LRC songs played over the original mix are playable immediately while their
  // key is still being detected off-queue. Until the key lands, treat the
  // key/tempo section as pending rather than showing controls.
  const keyPending =
    song.is_analyzed && song.transcript_source === "Lrc" && song.no_stems && song.key === null;
  const supportsShifts = song.is_analyzed && song.transcript_source !== "Usdx" && !keyPending;
  const supportsAnalysisActions = Boolean(status.isReady) && song.transcript_source !== "Usdx";

  // Off-queue key detection doesn't invalidate any query, so poll the song list
  // while the key is pending to pick it up and unlock the shift controls.
  useEffect(() => {
    if (!keyPending) return;
    const interval = setInterval(() => {
      queryClient.invalidateQueries({ queryKey: SONGS });
    }, 2000);
    return () => clearInterval(interval);
  }, [keyPending, queryClient]);

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
          shifting={shifting}
          setShifting={setShifting}
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
          className="h-8 w-full"
          disabled={!status.isReady || shifting.key || shifting.tempo}
          onClick={() => navigate("/playback", { state: { song } })}
        >
          <PlayIcon /> Play
        </Button>
      </footer>
    </aside>
  );
};
