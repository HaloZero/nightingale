import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useMenuFocus } from "@/contexts/menu-focus-context";
import { useAnalysis } from "@/hooks/use-analysis";
import { cn } from "@/lib/utils";
import { useSongs } from "@/queries/use-songs";
import {
  AlignLeftIcon,
  AudioLinesIcon,
  ImageIcon,
  MicIcon,
  RefreshCwIcon,
  Trash2Icon,
  EllipsisIcon,
} from "lucide-react";
import { useEffect, useState } from "react";

export const BulkActionsMenu = () => {
  const {
    enqueueAll,
    realignAll,
    reanalyzeAllFull,
    reanalyzeAllTranscript,
    reanalyzeAllForceTranscribe,
    refreshMetadataAll,
    deleteSongCacheAll,
  } = useAnalysis();
  const { data } = useSongs();
  const analyzedCount = data?.pages[0]?.analyzed_count ?? 0;

  const [open, setOpen] = useState(false);
  const { focus, actionsRef } = useMenuFocus();

  useEffect(() => {
    actionsRef.current.onConfirmActions = () => setOpen(true);
    return () => {
      actionsRef.current.onConfirmActions = null;
    };
  }, [actionsRef]);

  const isActionsFocused = focus.active && focus.panel === "songList" && focus.actionsFocused;

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button
          tabIndex={-1}
          variant="outline"
          size="icon"
          data-actions-focus="true"
          aria-label="Actions on filtered songs"
          title="Actions"
          className={cn(
            "border-input bg-input/20 focus-visible:border-transparent focus-visible:ring-0 dark:bg-input/30",
            isActionsFocused && "ring-2 ring-primary",
          )}
        >
          <EllipsisIcon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-56">
        <DropdownMenuLabel>All songs</DropdownMenuLabel>
        <DropdownMenuItem onClick={() => enqueueAll()}>
          <AudioLinesIcon />
          Analyze all
        </DropdownMenuItem>
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
