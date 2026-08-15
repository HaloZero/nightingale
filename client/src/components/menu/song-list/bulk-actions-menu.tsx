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
import { AudioLinesIcon, EllipsisIcon, ImageIcon, RefreshCwIcon } from "lucide-react";

/** Bulk counterpart to a subset of the per-song actions in song-actions.ts,
 * applied to every song matching the current library filter instead of one
 * song at a time. "Refresh metadata" applies to any local-file song
 * regardless of analysis state; "Full reanalysis" and "Refetch lyrics &
 * align" only apply to already-analyzed songs (mirrors the per-song menu's
 * gating), so they're grouped under their own section, hidden entirely when
 * the current filter has no analyzed songs. Ineligible songs within an
 * eligible section (USDX, LRC-provided, etc.) are still excluded
 * server-side per action; see the eligibility queries in app-core's
 * library_db/queries.rs. */
export const BulkActionsMenu = () => {
  const { reanalyzeAllFull, reanalyzeAllTranscript, refreshMetadataAll } = useAnalysis();
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
            <DropdownMenuItem onClick={() => reanalyzeAllTranscript()}>
              <RefreshCwIcon />
              Refetch lyrics & align
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => reanalyzeAllFull()}>
              <AudioLinesIcon />
              Full reanalysis
            </DropdownMenuItem>
          </>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};
