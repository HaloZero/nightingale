import type { FailureKind } from "@/types/FailureKind";

const LABELS: Record<FailureKind, string> = {
  GpuOom: "Out of GPU memory",
  AudioPrep: "Audio preparation failed",
  ServerStartup: "Analyzer server failed to start",
  ServerCrash: "Analyzer server crashed",
  MissingOutput: "Missing transcript output",
  Worker: "Analysis failed",
  Other: "Analysis failed",
};

export const labelForFailureKind = (kind: FailureKind): string => LABELS[kind];
