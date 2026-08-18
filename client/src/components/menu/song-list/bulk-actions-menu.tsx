import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useAnalysis } from "@/hooks/use-analysis";
import { useSongs } from "@/queries/use-songs";
import {
  AlignLeftIcon,
  AudioLinesIcon,
  EllipsisIcon,
  ImageIcon,
  MicIcon,
  RefreshCwIcon,
  Trash2Icon,
} from "lucide-react";

/** Mirrors song-actions.ts (same actions, wording, icons), minus "Edit
 * lyrics" / "Change language" which have no bulk equivalent. The analyzed
 * actions are gated and hidden as a group when there's nothing eligible, to
 * match the per-song menu's `supportsAnalysisActions` gating; per-action
 * exclusions (USDX, LRC-provided, etc.) still happen server-side, see
 * app-core's library_db/queries.rs. */
export const BulkActionsMenu = () => {
  const {
    realignAll,
    reanalyzeAllFull,
    reanalyzeAllTranscript,
    reanalyzeAllForceTranscribe,
    refreshMetadataAll,
    deleteSongCacheAll,
  } = useAnalysis();
  const { data } = useSongs();
  const analyzedCount = data?.pages[0]?.analyzed_count ?? 0;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          tabIndex={-1}
          variant="outline"
          aria-label="More actions on filtered songs"
          className="w-7 px-0 focus-visible:border-transparent focus-visible:ring-0"
        >
          <EllipsisIcon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-56">
        <DropdownMenuLabel>All songs</DropdownMenuLabel>
        <DropdownMenuItem onClick={() => refreshMetadataAll()}>
          <ImageIcon />
          Refresh metadata
        </DropdownMenuItem>
        {analyzedCount > 0 ? (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>Analyzed songs ({analyzedCount})</DropdownMenuLabel>
            <DropdownMenuItem onClick={() => realignAll()}>
              <AlignLeftIcon />
              Realign
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => reanalyzeAllTranscript()}>
              <RefreshCwIcon />
              Refetch lyrics & align
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => reanalyzeAllForceTranscribe()}>
              <MicIcon />
              Force transcribe
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => reanalyzeAllFull()}>
              <AudioLinesIcon />
              Full reanalysis
            </DropdownMenuItem>
            <DropdownMenuItem variant="destructive" onClick={() => deleteSongCacheAll()}>
              <Trash2Icon />
              Delete cache
            </DropdownMenuItem>
          </>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};
