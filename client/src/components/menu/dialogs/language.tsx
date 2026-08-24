import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldGroup } from "@/components/ui/field";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { DialogMode, useDialog } from "@/hooks/use-dialog";
import { useDialogNav } from "@/hooks/navigation/use-dialog-nav";
import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";
import { LANGUAGES } from "@/lib/languages";
import { realign, reanalyzeTranscript, songsByHashes } from "@/bridge/analysis";
import { Song } from "@/types/Song";

export function isLanguageDialogMode(mode: DialogMode): mode is { mode: "language"; song: Song } {
  return mode !== null && typeof mode === "object" && mode.mode === "language";
}

export const SelectLanguageDialog = () => {
  const { mode, close } = useDialog();
  const containerRef = useRef<HTMLDivElement>(null);

  const languageDialog = isLanguageDialogMode(mode) ? mode : null;
  const open = languageDialog !== null;
  const currentLanguage = languageDialog?.song.language ?? undefined;

  const [language, setLanguage] = useState<string | undefined>(currentLanguage);
  const [analysisMode, setAnalysisMode] = useState<"force" | "realign">("force");

  useEffect(() => {
    setLanguage(currentLanguage);
    setAnalysisMode("force");
  }, [currentLanguage, open]);

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

  if (!song.language) {
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
              <Select value={language} onValueChange={(language) => setLanguage(language)}>
                <SelectTrigger
                  id="language-select"
                  className={cn(
                    "focus-visible:ring-0 focus-visible:border-transparent",
                    focusedIndex === 0 && "ring-2 ring-primary",
                  )}
                >
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
                onValueChange={(mode) => setAnalysisMode(mode as "force" | "realign")}
              >
                <SelectTrigger
                  id="analysis-mode-select"
                  className={cn(
                    "focus-visible:ring-0 focus-visible:border-transparent",
                    focusedIndex === 1 && "ring-2 ring-primary",
                  )}
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
            </Field>
          </FieldGroup>
          <DialogFooter>
            <DialogClose asChild>
              <Button
                variant="outline"
                onClick={close}
                className={cn(
                  "focus-visible:ring-0 focus-visible:border-transparent",
                  focusedIndex === 2 && "ring-2 ring-primary",
                )}
              >
                Cancel
              </Button>
            </DialogClose>
            <Button
              disabled={!language || (language === song.language && analysisMode === "force")}
              onClick={() => {
                if (language) {
                  const target = songsByHashes([song.file_hash]);
                  if (analysisMode === "realign") {
                    realign(target, language);
                  } else {
                    reanalyzeTranscript(target, language);
                  }
                }

                close();
              }}
              className={cn(
                "focus-visible:ring-0 focus-visible:border-transparent",
                focusedIndex === 3 && "ring-2 ring-primary",
              )}
            >
              Select
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
};
