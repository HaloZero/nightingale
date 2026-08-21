export type AnalysisMode = "force" | "realign" | "set_only";

/** What each "Change language" mode actually does, since the two/three
 * options look similar but touch very different amounts of cached state --
 * shown under the mode picker in both the per-song and bulk dialogs. */
export const ANALYSIS_MODE_DESCRIPTIONS: Record<AnalysisMode, string> = {
  force:
    "Re-fetches lyrics (or transcribes from scratch if none are found) and re-aligns them in the new language. Use this when the words themselves are wrong.",
  realign:
    "Keeps the current transcript's words as-is and only re-times them against the vocals in the new language. Use this when the words are right but the timing is off.",
  set_only:
    "Just updates the stored language. Doesn't touch the transcript, cached lyrics, or stems -- nothing gets reprocessed.",
};
