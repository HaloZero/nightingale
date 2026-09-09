import { ListMusicIcon, PlayIcon, Trash2Icon, XIcon } from 'lucide-react';

import type { PlaybackQueueEntry } from '@/bridge/playback-queue';
import { useSongDetailsNav } from '@/features/library/components/song-list/details/use-song-details-nav';
import { AlbumArt } from '@/features/library/components/song-list/shared/album-art';
import { useDialog } from '@/features/menu/hooks/use-dialog';
import { useDialogNav } from '@/features/menu/hooks/use-dialog-nav';
import {
  useClearPlaybackQueue,
  useRemovePlaybackQueueEntry,
  useStartNextPlaybackQueueSong,
} from '@/features/playback-queue/use-playback-queue';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/shared/components/ui/alert-dialog';
import { Button } from '@/shared/components/ui/button';
import { Spinner } from '@/shared/components/ui/spinner';
import { cn } from '@/shared/utils/cn';

type QueueSidebarProps = {
  entries: PlaybackQueueEntry[];
  onClose: () => void;
};

const dialogFocusClass = (open: boolean, focusedIndex: number, index: number): string =>
  cn(
    'focus-visible:border-transparent focus-visible:ring-0',
    open && focusedIndex === index && 'ring-2 ring-primary',
  );

export function QueueSidebar({ entries, onClose }: QueueSidebarProps) {
  const { isPreparing, playNext } = useStartNextPlaybackQueueSong(entries);
  const { mutate: remove, isLoading: removing } = useRemovePlaybackQueueEntry();
  const { mutate: clear, isLoading: clearing } = useClearPlaybackQueue();
  const { mode, setMode, close: closeDialog } = useDialog();
  const { detailsRef, closeDetails } = useSongDetailsNav(onClose);
  const confirmOpen = mode === 'clear-playback-queue';
  const confirmClear = () => {
    clear();
    closeDialog();
  };
  const { focusedIndex } = useDialogNav({
    open: confirmOpen,
    itemCount: 2,
    onConfirm: (index) => (index === 0 ? closeDialog() : confirmClear()),
    onBack: closeDialog,
  });

  return (
    <aside
      ref={detailsRef}
      className="flex min-h-0 min-w-0 flex-1 flex-col border-l bg-background [&_[data-song-details-focused=true]]:z-10 [&_[data-song-details-focused=true]]:ring-2 [&_[data-song-details-focused=true]]:ring-primary xl:w-96 xl:flex-none"
      aria-label="Playback queue"
    >
      <header className="flex items-center gap-3 border-b p-3">
        <ListMusicIcon className="size-5" />
        <h2 className="min-w-0 flex-1 text-sm font-semibold">Playback Queue</h2>
        <Button variant="ghost" size="icon-sm" onClick={closeDetails} aria-label="Close queue">
          <XIcon />
        </Button>
      </header>

      <div className="no-scrollbar min-h-0 flex-1 overflow-y-auto p-3">
        {entries.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">Queue is empty</p>
        ) : (
          <ol className="divide-y">
            {entries.map((entry, index) => (
              <li key={entry.id} className="flex items-center gap-2 py-2 first:pt-0 last:pb-0">
                <AlbumArt song={entry.song} className="size-10 rounded-sm" />
                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 items-center text-sm font-medium">
                    <p className="truncate">{entry.song.title}</p>
                    {index === 0 ? (
                      <span className="shrink-0 text-xs font-medium text-primary"> · Next up</span>
                    ) : null}
                  </div>
                  <p className="truncate text-xs text-muted-foreground">
                    {entry.song.artist || '—'}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  disabled={removing || clearing}
                  onClick={() => remove(entry.id)}
                  aria-label={`Remove ${entry.song.title} from queue`}
                >
                  <Trash2Icon />
                </Button>
              </li>
            ))}
          </ol>
        )}
      </div>

      <footer
        className="flex gap-2 border-t p-3 pb-[max(0.75rem,env(safe-area-inset-bottom))]"
        data-song-details-nav-group
      >
        <Button
          size="lg"
          className="h-8 flex-1"
          disabled={entries.length === 0 || isPreparing || clearing}
          aria-busy={isPreparing}
          onClick={playNext}
        >
          {isPreparing ? (
            <>
              <Spinner className="size-4" /> Preparing playback…
            </>
          ) : (
            <>
              <PlayIcon /> Play Queue
            </>
          )}
        </Button>
        <AlertDialog
          open={confirmOpen}
          onOpenChange={(open) => setMode(open ? 'clear-playback-queue' : null)}
        >
          <AlertDialogTrigger asChild>
            <Button
              variant="destructive"
              size="icon-lg"
              disabled={entries.length === 0 || clearing || removing || isPreparing}
              aria-label="Clear playback queue"
              title="Clear queue"
            >
              <Trash2Icon />
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent onEscapeKeyDown={(event) => event.preventDefault()}>
            <AlertDialogHeader>
              <AlertDialogTitle>Clear playback queue?</AlertDialogTitle>
              <AlertDialogDescription>
                This removes all {entries.length} {entries.length === 1 ? 'song' : 'songs'} from the
                queue.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel
                className={dialogFocusClass(confirmOpen, focusedIndex, 0)}
                onClick={closeDialog}
              >
                Cancel
              </AlertDialogCancel>
              <AlertDialogAction
                variant="destructive"
                className={dialogFocusClass(confirmOpen, focusedIndex, 1)}
                onClick={confirmClear}
              >
                Clear queue
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </footer>
    </aside>
  );
}
