import { useRef, useState } from 'react';

import { realign, reanalyzeTranscript } from '@/bridge/analysis';
import { LANGUAGES } from '@/features/lyrics/lib/languages';
import type { DialogMode } from '@/features/menu/hooks/use-dialog';
import { useDialog } from '@/features/menu/hooks/use-dialog';
import { useDialogNav } from '@/features/menu/hooks/use-dialog-nav';
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
import type { Song } from '@/types/Song';

type LanguageSelection = {
  source: string | undefined;
  language: string | undefined;
  analysisMode: 'force' | 'realign';
};

const currentSelection = (
  selection: LanguageSelection,
  source: string | undefined,
): Pick<LanguageSelection, 'language' | 'analysisMode'> =>
  selection.source === source ? selection : { language: source, analysisMode: 'force' };

const selectableSong = (song: Song): boolean =>
  typeof song.language === 'string' && song.language !== '';

const focusRing = (focusedIndex: number, index: number): string =>
  cn(
    'focus-visible:ring-0 focus-visible:border-transparent',
    focusedIndex === index && 'ring-2 ring-primary',
  );

export function isLanguageDialogMode(mode: DialogMode): mode is { mode: 'language'; song: Song } {
  return mode !== null && typeof mode === 'object' && mode.mode === 'language';
}

export const SelectLanguageDialog = () => {
  const { mode, close } = useDialog();
  const containerRef = useRef<HTMLDivElement>(null);

  const languageDialog = isLanguageDialogMode(mode) ? mode : null;
  const open = languageDialog !== null;
  const currentLanguage = languageDialog?.song.language ?? undefined;

  const [selection, setSelection] = useState<LanguageSelection>({
    source: currentLanguage,
    language: currentLanguage,
    analysisMode: 'force',
  });
  const { language, analysisMode } = currentSelection(selection, currentLanguage);

  const { focusedIndex } = useDialogNav({
    open,
    itemCount: 4,
    onBack: close,
    containerRef,
  });

  if (!languageDialog) {
    return null;
  }

  const { song } = languageDialog;

  if (!selectableSong(song)) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="sm:max-w-sm">
        <div ref={containerRef} className="contents">
          <DialogHeader>
            <DialogTitle>Select Language</DialogTitle>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <Label htmlFor="language-select">Language</Label>
              <Select
                value={language}
                onValueChange={(nextLanguage) =>
                  setSelection({ source: currentLanguage, language: nextLanguage, analysisMode })
                }
              >
                <SelectTrigger id="language-select" className={focusRing(focusedIndex, 0)}>
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
              <Label htmlFor="analysis-mode-select">Mode</Label>
              <Select
                value={analysisMode}
                onValueChange={(nextMode) => {
                  if (nextMode === 'force' || nextMode === 'realign') {
                    setSelection({ source: currentLanguage, language, analysisMode: nextMode });
                  }
                }}
              >
                <SelectTrigger id="analysis-mode-select" className={focusRing(focusedIndex, 1)}>
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
            </Field>
          </FieldGroup>
          <DialogFooter>
            <DialogClose asChild>
              <Button variant="outline" onClick={close} className={focusRing(focusedIndex, 2)}>
                Cancel
              </Button>
            </DialogClose>
            <Button
              disabled={
                typeof language !== 'string' ||
                language === '' ||
                (language === song.language && analysisMode === 'force')
              }
              onClick={() => {
                if (typeof language === 'string' && language !== '') {
                  if (analysisMode === 'realign') {
                    void realign(song.file_hash, language);
                  } else {
                    void reanalyzeTranscript(song.file_hash, language);
                  }
                }

                close();
              }}
              className={focusRing(focusedIndex, 3)}
            >
              Select
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
};
