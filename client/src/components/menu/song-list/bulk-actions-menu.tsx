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
import { useDialog } from "@/hooks/use-dialog";
import {
  AlignLeftIcon,
  AudioLinesIcon,
  EllipsisIcon,
  ImageIcon,
  LanguagesIcon,
  ListXIcon,
  MicIcon,
  RefreshCwIcon,
} from "lucide-react";

/** Bulk counterpart to the per-song "Realign / Refetch lyrics & align /
 * Force transcribe / Full reanalysis / Change language / Refresh metadata /
 * Remove from queue" actions in song-actions.ts, applied to every song
 * matching the current library filter instead of one song at a time.
 * Ineligible songs (not yet analyzed, USDX, or -- for everything but full
 * reanalysis and refresh metadata -- LRC-provided) are excluded
 * server-side per action; see the eligibility queries in app-core's
 * library_db/queries.rs. */
export const BulkActionsMenu = () => {
  const {
    reanalyzeAllFull,
    reanalyzeAllTranscript,
    reanalyzeAllForceTranscribe,
    realignAll,
    refreshMetadataAll,
    removeFromQueueAll,
  } = useAnalysis();
  const { setMode } = useDialog();

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
        <DropdownMenuLabel>Filtered songs</DropdownMenuLabel>
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
        <DropdownMenuItem onClick={() => setMode({ mode: "bulk-language" })}>
          <LanguagesIcon />
          Change language
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => refreshMetadataAll()}>
          <ImageIcon />
          Refresh metadata
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => removeFromQueueAll()}>
          <ListXIcon />
          Remove from queue
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
};
