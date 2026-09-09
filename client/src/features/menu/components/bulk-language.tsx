import { useRef, useState } from 'react';

import { useAnalysis } from '@/features/library/hooks/use-analysis';
import { LANGUAGES } from '@/features/lyrics/lib/languages';
import type { DialogMode } from '@/features/menu/hooks/use-dialog';
import { useDialog } from '@/features/menu/hooks/use-dialog';
import { useDialogNav } from '@/features/menu/hooks/use-dialog-nav';
import { ANALYSIS_MODE_DESCRIPTIONS } from '@/features/playback/lib/analysis-mode';
import { Button } from '@/shared/components/ui/button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/shared/components/ui/dialog';
import { Field, FieldGroup } from '@/shared/components/ui/field';
import { Label } from '@/shared/components/ui/label';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/shared/components/ui/select';
import { cn } from '@/shared/utils/cn';

function isBulkLanguageDialogMode(mode: DialogMode): mode is { mode: 'bulk-language' } {
  return mode !== null && typeof mode === 'object' && mode.mode === 'bulk-language';
}

type BulkLanguageSelection = {
  open: boolean;
  language: string | undefined;
  analysisMode: 'force' | 'realign';
};

// Resets to a blank selection every time the dialog transitions from closed
// to open -- derived during render (like `SelectLanguageDialog`'s
// `currentSelection`) rather than via an effect, so there's no separate
// synchronous `setState` render pass on open.
const currentSelection = (
  selection: BulkLanguageSelection,
  open: boolean,
): Pick<BulkLanguageSelection, 'language' | 'analysisMode'> =>
  selection.open === open ? selection : { language: undefined, analysisMode: 'force' };

const focusRing = (focusedIndex: number, index: number): string =>
  cn(
    'focus-visible:ring-0 focus-visible:border-transparent',
    focusedIndex === index && 'ring-2 ring-primary',
  );

const isAnalysisMode = (value: string): value is 'force' | 'realign' =>
  value === 'force' || value === 'realign';

/** Bulk counterpart to SelectLanguageDialog: applies to every song matching
 * the currently active library filter (not one song), via
 * reanalyzeAllTranscript/realignAll -- same two bulk endpoints "Refetch
 * lyrics & align"/"Realign" already use, just with a language override. No
 * per-song `currentLanguage` to prefill from, since songs in the filter can
 * already have different languages. */
export const BulkSelectLanguageDialog = () => {
  const { mode, close } = useDialog();
  const containerRef = useRef<HTMLDivElement>(null);
  const { reanalyzeAllTranscript, realignAll } = useAnalysis();

  const open = isBulkLanguageDialogMode(mode);

  const [selection, setSelection] = useState<BulkLanguageSelection>({
    open,
    language: undefined,
    analysisMode: 'force',
  });
  const { language, analysisMode } = currentSelection(selection, open);

  const { focusedIndex } = useDialogNav({
    open,
    itemCount: 4,
    onBack: close,
    containerRef,
  });

  if (!open) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="sm:max-w-sm">
        <div ref={containerRef} className="contents">
          <DialogHeader>
            <DialogTitle>Change Language (Filtered Songs)</DialogTitle>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <Label htmlFor="bulk-language-select">Language</Label>
              <Select
                value={language}
                onValueChange={(nextLanguage) =>
                  setSelection({ open, language: nextLanguage, analysisMode })
                }
              >
                <SelectTrigger id="bulk-language-select" className={focusRing(focusedIndex, 0)}>
                  <SelectValue placeholder="Select language" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectLabel>Language</SelectLabel>
                    {LANGUAGES.map(([value, label]) => (
                      <SelectItem key={value} value={value}>
                        {label}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
            <Field>
              <Label htmlFor="bulk-analysis-mode-select">Mode</Label>
              <Select
                value={analysisMode}
                onValueChange={(nextMode) => {
                  if (isAnalysisMode(nextMode)) {
                    setSelection({ open, language, analysisMode: nextMode });
                  }
                }}
              >
                <SelectTrigger
                  id="bulk-analysis-mode-select"
                  className={focusRing(focusedIndex, 1)}
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectLabel>Mode</SelectLabel>
                    <SelectItem value="force">Force transcript</SelectItem>
                    <SelectItem value="realign">Realign saved lyrics</SelectItem>
                  </SelectGroup>
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                {ANALYSIS_MODE_DESCRIPTIONS[analysisMode]}
              </p>
            </Field>
          </FieldGroup>
          <DialogFooter>
            <DialogClose asChild>
              <Button variant="outline" onClick={close} className={focusRing(focusedIndex, 2)}>
                Cancel
              </Button>
            </DialogClose>
            <Button
              disabled={language === undefined}
              onClick={() => {
                if (language !== undefined) {
                  if (analysisMode === 'realign') {
                    void realignAll(language);
                  } else {
                    void reanalyzeAllTranscript(language);
                  }
                }

                close();
              }}
              className={focusRing(focusedIndex, 3)}
            >
              Apply to filtered songs
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
};
