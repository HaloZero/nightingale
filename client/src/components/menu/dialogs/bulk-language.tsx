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
import { useAnalysis } from "@/hooks/use-analysis";
import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";
import { LANGUAGES } from "@/lib/languages";
import { ANALYSIS_MODE_DESCRIPTIONS } from "@/lib/analysis-mode";

export function isBulkLanguageDialogMode(mode: DialogMode): mode is { mode: "bulk-language" } {
  return mode !== null && typeof mode === "object" && mode.mode === "bulk-language";
}

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

  const [language, setLanguage] = useState<string | undefined>(undefined);
  const [analysisMode, setAnalysisMode] = useState<"force" | "realign">("force");

  useEffect(() => {
    setLanguage(undefined);
    setAnalysisMode("force");
  }, [open]);

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
              <Select value={language} onValueChange={(language) => setLanguage(language)}>
                <SelectTrigger
                  id="bulk-language-select"
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
              <Label htmlFor="bulk-analysis-mode-select">Mode</Label>
              <Select
                value={analysisMode}
                onValueChange={(mode) => setAnalysisMode(mode as "force" | "realign")}
              >
                <SelectTrigger
                  id="bulk-analysis-mode-select"
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
              <p className="text-xs text-muted-foreground">
                {ANALYSIS_MODE_DESCRIPTIONS[analysisMode]}
              </p>
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
              disabled={!language}
              onClick={() => {
                if (language) {
                  if (analysisMode === "realign") {
                    realignAll(language);
                  } else {
                    reanalyzeAllTranscript(language);
                  }
                }

                close();
              }}
              className={cn(
                "focus-visible:ring-0 focus-visible:border-transparent",
                focusedIndex === 3 && "ring-2 ring-primary",
              )}
            >
              Apply to filtered songs
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
};
