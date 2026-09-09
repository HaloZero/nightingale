import {
  AlignLeftIcon,
  AudioLinesIcon,
  EllipsisIcon,
  ImageIcon,
  LanguagesIcon,
  ListXIcon,
  MicIcon,
  RefreshCwIcon,
  Repeat2Icon,
  VideoIcon,
  XCircleIcon,
} from 'lucide-react';
import { useEffect, useState } from 'react';

import { isTauri } from '@/bridge/runtime';
import { useAnalysis } from '@/features/library/hooks/use-analysis';
import { useSongs } from '@/features/library/queries/use-songs';
import { useDialog } from '@/features/menu/hooks/use-dialog';
import { useMenuFocus, type MenuFocus } from '@/features/menu/providers/menu-focus-context';
import { Button } from '@/shared/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/shared/components/ui/dropdown-menu';
import { cn } from '@/shared/utils/cn';

type CancelAnalysisItemProps = {
  count: number;
  onClick: () => void;
};

function isSongListActionsFocused(focus: MenuFocus): boolean {
  return (
    focus.active && focus.panel === 'songList' && focus.actionsFocused && focus.actionsIndex === 1
  );
}

const CancelAnalysisItem = ({ count, onClick }: CancelAnalysisItemProps) => {
  if (count === 0) {
    return null;
  }

  return (
    <DropdownMenuItem variant="destructive" onClick={onClick}>
      <XCircleIcon />
      Cancel analysis ({count})
    </DropdownMenuItem>
  );
};

/** Bulk counterpart to the per-song "Realign / Refetch lyrics & align /
 * Force transcribe / Full reanalysis / Change language / Refresh metadata /
 * Remove from queue / Render karaoke video / Force re-render karaoke video"
 * actions in song-actions.ts, applied to every song matching the current
 * library filter instead of one song at a time. Ineligible songs (not yet
 * analyzed, USDX, or -- for everything but full reanalysis and refresh
 * metadata -- LRC-provided) are excluded server-side per action; see the
 * eligibility queries in app-core's library_db/queries.rs. The karaoke
 * video actions are additionally hidden in the Tauri desktop build
 * (`!isTauri`) -- same as their per-song counterparts, see
 * bridge/karaoke-video.ts. */
export const BulkActionsMenu = () => {
  const {
    enqueueAll,
    cancelAnalysisAll,
    realignAll,
    reanalyzeAllTranscript,
    reanalyzeAllForceTranscribe,
    reanalyzeAllFull,
    refreshMetadataAll,
    removeFromQueueAll,
    bestKaraokeVideoAll,
    forceBestKaraokeVideoAll,
  } = useAnalysis();
  const { data } = useSongs();
  const { analyzed_count: analyzedCount = 0, analysis_busy_count: analysisBusyCount = 0 } =
    data?.pages[0] ?? {};
  const { setMode } = useDialog();

  const [open, setOpen] = useState(false);
  const { focus, actionsRef } = useMenuFocus();

  useEffect(() => {
    const actions = actionsRef.current;
    actions.onConfirmActions = (index) => {
      if (index !== 1) {
        return false;
      }
      setOpen(true);
      return true;
    };

    return () => {
      actions.onConfirmActions = null;
    };
  }, [actionsRef]);

  const isActionsFocused = isSongListActionsFocused(focus);

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="icon"
          data-actions-index="1"
          aria-label="Actions on filtered songs"
          title="Actions"
          className={cn(
            'border-input bg-input/20 focus-visible:border-transparent focus-visible:ring-0 dark:bg-input/30',
            isActionsFocused && 'ring-2 ring-primary',
          )}
        >
          <EllipsisIcon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-56">
        <DropdownMenuLabel>All songs</DropdownMenuLabel>
        <DropdownMenuItem onClick={() => void enqueueAll()}>
          <AudioLinesIcon />
          Analyze all
        </DropdownMenuItem>
        <CancelAnalysisItem count={analysisBusyCount} onClick={() => void cancelAnalysisAll()} />
        <DropdownMenuItem onClick={() => void refreshMetadataAll()}>
          <ImageIcon />
          Refresh metadata
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => void removeFromQueueAll()}>
          <ListXIcon />
          Remove from queue
        </DropdownMenuItem>
        {analyzedCount > 0 ? (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>Analyzed songs ({analyzedCount})</DropdownMenuLabel>
            <DropdownMenuItem onClick={() => void realignAll()}>
              <AlignLeftIcon />
              Realign
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => void reanalyzeAllTranscript()}>
              <RefreshCwIcon />
              Refetch lyrics & align
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => void reanalyzeAllForceTranscribe()}>
              <MicIcon />
              Force transcribe
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => void reanalyzeAllFull()}>
              <AudioLinesIcon />
              Full reanalysis
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setMode({ mode: 'bulk-language' })}>
              <LanguagesIcon />
              Change language
            </DropdownMenuItem>
          </>
        ) : null}
        {!isTauri && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => void bestKaraokeVideoAll()}>
              <VideoIcon />
              Render karaoke video
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => void forceBestKaraokeVideoAll()}>
              <Repeat2Icon />
              Force re-render karaoke video
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};
