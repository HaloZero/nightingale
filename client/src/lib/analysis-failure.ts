// Analysis failures only carry a free-form message from app-core (see
// QueuedStatus::Failed call sites in app-core/src/analyzer.rs); there's no
// structured error kind to key off, so classification here is matched
// against those exact message shapes.
export type AnalysisFailureCategory = {
  id: string;
  label: string;
};

const OTHER_CATEGORY: AnalysisFailureCategory = { id: "other", label: "Analysis failed" };

const CATEGORIES: (AnalysisFailureCategory & { matches: (message: string) => boolean })[] = [
  {
    id: "gpu-oom",
    label: "Out of GPU memory",
    matches: (message) => message === "CUDA out of memory",
  },
  {
    id: "audio-prep",
    label: "Audio preparation failed",
    matches: (message) => message.startsWith("audio prep failed:"),
  },
  {
    id: "server-crash",
    label: "Analyzer server crashed",
    matches: (message) => message.startsWith("Server crashed:"),
  },
  {
    id: "missing-output",
    label: "Missing transcript output",
    matches: (message) => message === "Transcript file not found after analysis",
  },
];

export const classifyAnalysisFailure = (message: string): AnalysisFailureCategory =>
  CATEGORIES.find((category) => category.matches(message)) ?? OTHER_CATEGORY;
