import { useDialogNav } from '@/features/menu/hooks/use-dialog-nav';
import { topScoresForSong } from '@/features/playback/utils/result';
import { Stars } from '@/shared/components/shared/stars';
import { Button } from '@/shared/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/shared/components/ui/dialog';
import { Spinner } from '@/shared/components/ui/spinner';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/shared/components/ui/table';
import { cn } from '@/shared/utils/cn';
import type { ScoreRecord } from '@/types/ScoreRecord';
import type { Song } from '@/types/Song';

const TOP_LIMIT = 5;

const RING = 'ring-2 ring-primary';
const NO_FOCUS_RING = 'focus-visible:ring-0 focus-visible:border-transparent';

type Props = {
  open: boolean;
  score: number;
  song: Song;
  scores: ScoreRecord[];
  activeProfile: string | null;
  nextPending: boolean;
  exitLabel: string;
  onBack: () => void;
  onNext?: () => void;
};

export const ResultDialog = ({
  open,
  score,
  song,
  scores,
  activeProfile,
  nextPending,
  exitLabel,
  onBack,
  onNext,
}: Props) => {
  const board = topScoresForSong(scores, song.file_hash, TOP_LIMIT);

  const { focusedIndex } = useDialogNav({
    open,
    itemCount: onNext ? 2 : 1,
    onConfirm: (index) => (index === 0 ? onBack() : onNext?.()),
    onBack,
  });

  return (
    <Dialog open={open} modal>
      <DialogContent
        showCloseButton={false}
        className="overflow-visible p-0 sm:max-w-sm"
        onEscapeKeyDown={(e) => e.preventDefault()}
        onPointerDownOutside={(e) => e.preventDefault()}
      >
        <div className="flex flex-col gap-4 p-7">
          <DialogHeader className="gap-1 text-center sm:text-center">
            <DialogTitle className="text-xl font-semibold">{song.title}</DialogTitle>
            <DialogDescription className="text-sm">{song.artist}</DialogDescription>
          </DialogHeader>

          <div className="flex flex-col items-center gap-1">
            <p
              className="text-4xl font-semibold text-primary tabular-nums"
              aria-label={`Score ${score}`}
            >
              {score}
            </p>
            <Stars score={score} size="lg" className="mt-1" />
          </div>

          {board.length > 0 ? (
            <>
              <div className="h-px w-full bg-border" />
              <p className="text-center text-[11px] font-medium tracking-wide text-muted-foreground">
                BEST SCORES
              </p>
              <Table>
                <TableHeader>
                  <TableRow className="hover:bg-transparent">
                    <TableHead className="h-8 text-xs">#</TableHead>
                    <TableHead className="h-8 text-xs">Profile</TableHead>
                    <TableHead className="h-8 text-right text-xs">Score</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {board.map(({ profile, score: rowScore }, i) => {
                    const isCurrent = profile === activeProfile && rowScore === score;

                    return (
                      <TableRow key={profile} className={cn(isCurrent && 'bg-primary/10')}>
                        <TableCell className="py-2 text-xs tabular-nums">{i + 1}</TableCell>
                        <TableCell
                          className={cn('py-2 text-xs', isCurrent && 'font-medium text-primary')}
                        >
                          {profile}
                        </TableCell>
                        <TableCell
                          className={cn(
                            'py-2 text-right text-xs tabular-nums',
                            isCurrent && 'font-medium text-primary',
                          )}
                        >
                          {rowScore}
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
            </>
          ) : null}

          <DialogFooter className="mt-2 sm:justify-center">
            <Button
              type="button"
              variant="outline"
              className={cn('w-full sm:w-auto', NO_FOCUS_RING, open && focusedIndex === 0 && RING)}
              disabled={nextPending}
              onClick={onBack}
            >
              {exitLabel}
            </Button>
            {onNext ? (
              <Button
                type="button"
                className={cn(
                  'w-full sm:w-auto',
                  NO_FOCUS_RING,
                  open && focusedIndex === 1 && RING,
                )}
                disabled={nextPending}
                aria-busy={nextPending}
                onClick={onNext}
              >
                {nextPending ? (
                  <>
                    <Spinner className="size-4" /> Preparing…
                  </>
                ) : (
                  'Next Song'
                )}
              </Button>
            ) : null}
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
};
