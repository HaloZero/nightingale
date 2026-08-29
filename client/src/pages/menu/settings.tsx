import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { Field, FieldGroup } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { setFullScreen, isFullScreen as tauriIsFullScreen } from "@/bridge/fullScreen";
import { pingParallelAnalysis } from "@/bridge/analysis";
import {
  type CountAndCap,
  buildBackgroundReels,
  downloadAllPixabayVideos,
  getBackgroundReelCount,
  getBackgroundVideoCount,
  onBackgroundReelsDone,
  onBackgroundReelsProgress,
  onPixabayBulkDownloadDone,
  onPixabayBulkDownloadProgress,
} from "@/bridge/background-videos";
import { isTauri } from "@/bridge/runtime";
import { useMicDevices } from "@/queries/use-mic-devices";
import { useConfigMutation } from "@/mutations/use-config-mutation";
import { useConfig } from "@/queries/use-config";
import type { AppConfig } from "@/types/AppConfig";
import { CheckCircle2Icon, Loader2Icon, XCircleIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import {
  ALIGN_BACKENDS,
  ASR_ENGINES,
  BACKGROUND_VIDEO_FLAVORS,
  DEFAULTS,
  LYRICS_HORIZONTAL_POSITIONS,
  LYRICS_VERTICAL_POSITIONS,
  MODELS,
  NAV,
  SEPARATORS,
  SETTINGS_TABS,
  VOCAL_THRESHOLD_MAX,
  getAnalysisNav,
  type SettingsTab,
} from "@/components/menu/settings/constants";
import { MicLatencyField } from "@/components/menu/settings/mic-latency-field";
import {
  Hint,
  NumberButtonGroup,
  PageHeader,
  SettingsSelect,
} from "@/components/menu/settings/settings-controls";
import { useSettingsNavigation } from "@/hooks/navigation/use-settings-navigation";

const DEFAULT_MIC_ID = "__default__";

export const SettingsPage = () => {
  const micDevices = useMicDevices();
  const navigate = useNavigate();
  const { data: config } = useConfig();
  const { mutate } = useConfigMutation();

  const containerRef = useRef<HTMLDivElement>(null);
  const [tab, setTab] = useState<SettingsTab>("general");
  const [isFullScreen, setIsFullScreen] = useState<boolean | null | undefined>(config?.fullscreen);
  const [micMonitorGain, setMicMonitorGain] = useState(
    config?.mic_monitor_gain ?? DEFAULTS.mic_monitor_gain,
  );
  const [micLatencySec, setMicLatencySec] = useState(
    config?.mic_latency_compensation_sec ?? DEFAULTS.mic_latency_compensation_sec,
  );
  const [vocalThresholdPct, setVocalThresholdPct] = useState(
    config?.vocal_detection_threshold_pct ?? DEFAULTS.vocal_detection_threshold_pct,
  );
  const [parallelUrl, setParallelUrl] = useState(
    config?.parallel_analysis_url ?? DEFAULTS.parallel_analysis_url,
  );
  const [pingStatus, setPingStatus] = useState<"idle" | "loading" | "alive" | "unreachable">(
    "idle",
  );
  // Keyed by flavor (`BACKGROUND_VIDEO_FLAVORS`) -- each flavor's download
  // and reel build run independently, so their status/progress can't share
  // a single value the way the rest of this page's flat state does.
  const [pixabayDownloadStatus, setPixabayDownloadStatus] = useState<
    Record<string, "idle" | "running" | "done">
  >({});
  const [pixabayDownloadMessage, setPixabayDownloadMessage] = useState<Record<string, string>>({});
  const [reelBuildStatus, setReelBuildStatus] = useState<
    Record<string, "idle" | "running" | "done">
  >({});
  const [reelBuildMessage, setReelBuildMessage] = useState<Record<string, string>>({});
  const [videoCounts, setVideoCounts] = useState<Record<string, CountAndCap>>({});
  const [reelCounts, setReelCounts] = useState<Record<string, CountAndCap>>({});

  const close = () => navigate("/");
  const asrEngine = config?.asr_engine ?? DEFAULTS.asr_engine;
  const isParakeet = asrEngine === "parakeet";
  // Parallel analysis offloads work to another self-hosted Nightingale
  // instance over HTTP -- there's nothing for the Tauri desktop app to point
  // at itself, so the section (and its nav segments, see `getSettingsStops`)
  // only exists in the web/server build.
  const showParallelAnalysis = !isTauri;
  const analysisNav = getAnalysisNav(isParakeet, showParallelAnalysis);
  // Both actions shell out to a vendored ffmpeg against the server's own
  // data dir -- there's no Tauri-side command for either, so this section
  // (and its nav segment, see `getSettingsStops`) is server-build only too.
  const showBackgroundVideos = !isTauri;

  const micOptions = useMemo(
    () => [
      { value: DEFAULT_MIC_ID, label: "Default" },
      ...micDevices.map(({ deviceId, label }) => ({ value: deviceId, label })),
    ],
    [micDevices],
  );
  const modelOptions = useMemo(() => MODELS.map((model) => ({ value: model, label: model })), []);
  const micMonitorGainPct = Math.round(micMonitorGain * 100);
  const vocalThresholdDisplayPct = Math.round(vocalThresholdPct * 100);
  const batchSize = config?.batch_size ?? DEFAULTS.batch_size;
  const beamSize = config?.beam_size ?? DEFAULTS.beam_size;

  useEffect(() => {
    setMicMonitorGain(config?.mic_monitor_gain ?? DEFAULTS.mic_monitor_gain);
  }, [config?.mic_monitor_gain]);

  useEffect(() => {
    setMicLatencySec(config?.mic_latency_compensation_sec ?? DEFAULTS.mic_latency_compensation_sec);
  }, [config?.mic_latency_compensation_sec]);

  useEffect(() => {
    setVocalThresholdPct(
      config?.vocal_detection_threshold_pct ?? DEFAULTS.vocal_detection_threshold_pct,
    );
  }, [config?.vocal_detection_threshold_pct]);

  useEffect(() => {
    setParallelUrl(config?.parallel_analysis_url ?? DEFAULTS.parallel_analysis_url);
  }, [config?.parallel_analysis_url]);

  useEffect(() => {
    const updateIsFullScreen = async () => {
      setIsFullScreen(await tauriIsFullScreen());
    };

    updateIsFullScreen();
  }, []);

  const refreshVideoCount = (flavor: string) => {
    getBackgroundVideoCount(flavor).then((result) => {
      setVideoCounts((prev) => ({ ...prev, [flavor]: result }));
    });
  };

  const refreshReelCount = (flavor: string) => {
    getBackgroundReelCount(flavor).then((result) => {
      setReelCounts((prev) => ({ ...prev, [flavor]: result }));
    });
  };

  useEffect(() => {
    if (!showBackgroundVideos) return;

    for (const { value: flavor } of BACKGROUND_VIDEO_FLAVORS) {
      refreshVideoCount(flavor);
      refreshReelCount(flavor);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showBackgroundVideos]);

  useEffect(() => {
    if (!showBackgroundVideos) return;

    let unlistenDownloadProgress: (() => void) | undefined;
    let unlistenDownloadDone: (() => void) | undefined;
    let unlistenReelsProgress: (() => void) | undefined;
    let unlistenReelsDone: (() => void) | undefined;

    onPixabayBulkDownloadProgress(({ flavor, message }) => {
      setPixabayDownloadMessage((prev) => ({ ...prev, [flavor]: message }));
    }).then((fn) => {
      unlistenDownloadProgress = fn;
    });
    onPixabayBulkDownloadDone(({ flavor }) => {
      setPixabayDownloadStatus((prev) => ({ ...prev, [flavor]: "done" }));
      setPixabayDownloadMessage((prev) => ({ ...prev, [flavor]: "Download complete." }));
      refreshVideoCount(flavor);
    }).then((fn) => {
      unlistenDownloadDone = fn;
    });
    onBackgroundReelsProgress(({ flavor, message }) => {
      setReelBuildMessage((prev) => ({ ...prev, [flavor]: message }));
    }).then((fn) => {
      unlistenReelsProgress = fn;
    });
    onBackgroundReelsDone(({ flavor }) => {
      setReelBuildStatus((prev) => ({ ...prev, [flavor]: "done" }));
      setReelBuildMessage((prev) => ({ ...prev, [flavor]: "Reels built." }));
      refreshReelCount(flavor);
    }).then((fn) => {
      unlistenReelsDone = fn;
    });

    return () => {
      unlistenDownloadProgress?.();
      unlistenDownloadDone?.();
      unlistenReelsProgress?.();
      unlistenReelsDone?.();
    };
  }, [showBackgroundVideos]);

  const updateMicMonitorGain = (gain: number) => {
    setMicMonitorGain(gain);
    mutate({ mic_monitor_gain: gain });
  };

  const updateMicLatency = (latencySec: number) => {
    setMicLatencySec(latencySec);
    mutate({ mic_latency_compensation_sec: latencySec });
  };

  const updateVocalThreshold = (pct: number) => {
    setVocalThresholdPct(pct);
    mutate({ vocal_detection_threshold_pct: pct });
  };

  const toggleWindowMode = (fullscreen: boolean) => {
    setIsFullScreen(fullscreen);
    setFullScreen(fullscreen);
    mutate({ fullscreen });
  };

  const commitParallelUrl = () => {
    const trimmed = parallelUrl.trim();
    setParallelUrl(trimmed);
    if (trimmed !== (config?.parallel_analysis_url ?? "")) {
      mutate({ parallel_analysis_url: trimmed || null });
    }
  };

  const pingParallel = async () => {
    const url = parallelUrl.trim();
    if (pingStatus === "loading" || url.length === 0) return;
    // Also persist whatever's being tested -- if you're pinging it, you
    // want it saved, and it means the field never silently drifts from
    // what Ping last confirmed reachable.
    commitParallelUrl();
    setPingStatus("loading");
    try {
      const alive = await pingParallelAnalysis(url);
      setPingStatus(alive ? "alive" : "unreachable");
    } catch {
      setPingStatus("unreachable");
    }
  };

  const startPixabayDownload = (flavor: string) => {
    if (pixabayDownloadStatus[flavor] === "running") return;
    setPixabayDownloadStatus((prev) => ({ ...prev, [flavor]: "running" }));
    setPixabayDownloadMessage((prev) => ({ ...prev, [flavor]: "Starting download..." }));
    downloadAllPixabayVideos(flavor);
  };

  const startReelBuild = (flavor: string) => {
    if (reelBuildStatus[flavor] === "running") return;
    setReelBuildStatus((prev) => ({ ...prev, [flavor]: "running" }));
    setReelBuildMessage((prev) => ({ ...prev, [flavor]: "Starting reel build..." }));
    buildBackgroundReels(flavor);
  };

  const resetDefaults = () => {
    mutate(DEFAULTS);
    setMicMonitorGain(DEFAULTS.mic_monitor_gain);
    setMicLatencySec(DEFAULTS.mic_latency_compensation_sec);
    setVocalThresholdPct(DEFAULTS.vocal_detection_threshold_pct);
    setParallelUrl(DEFAULTS.parallel_analysis_url);
    setPingStatus("idle");
  };

  const { footerSegment, getFocusClassName, syncFocusFromElement } = useSettingsNavigation({
    containerRef,
    tab,
    isParakeet,
    showParallelAnalysis,
    showBackgroundVideos,
    micMonitorGain,
    micLatencySec,
    vocalThresholdPct,
    onBack: close,
    onTabChange: setTab,
    onMicMonitorGainChange: updateMicMonitorGain,
    onMicLatencyChange: updateMicLatency,
    onVocalThresholdChange: updateVocalThreshold,
  });

  return (
    <div
      ref={containerRef}
      className="h-full overflow-y-auto px-4 pb-5 pt-14 sm:px-6 md:pt-5 lg:px-8"
      onMouseMoveCapture={(event) => syncFocusFromElement(event.target)}
      onFocusCapture={(event) => syncFocusFromElement(event.target)}
    >
      <div className="mx-auto flex max-w-4xl flex-col gap-5">
        <PageHeader />

        <Tabs value={tab} onValueChange={(value) => setTab(value as SettingsTab)}>
          <TabsList className="scrollbar-hide max-w-full overflow-x-auto overflow-y-hidden sm:w-fit">
            {SETTINGS_TABS.map((settingsTab, slot) => (
              <TabsTrigger
                key={settingsTab.value}
                value={settingsTab.value}
                className={getFocusClassName(NAV.tabSegment, slot)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") setTab(settingsTab.value);
                }}
              >
                {settingsTab.label}
              </TabsTrigger>
            ))}
          </TabsList>

          <TabsContent value="general" className="mt-4">
            <FieldGroup>
              <Field>
                <Label>Window</Label>
                <ButtonGroup>
                  <Button
                    variant={isFullScreen === true ? "outline" : "default"}
                    onClick={() => toggleWindowMode(false)}
                    className={getFocusClassName(NAV.general.window, 0)}
                  >
                    Windowed
                  </Button>
                  <Button
                    variant={isFullScreen === false ? "outline" : "default"}
                    onClick={() => toggleWindowMode(true)}
                    className={getFocusClassName(NAV.general.window, 1)}
                  >
                    Fullscreen
                  </Button>
                </ButtonGroup>
              </Field>

              <Field>
                <Label>Microphone</Label>
                <Hint>Select which microphone to use for pitch scoring</Hint>
                <SettingsSelect
                  label="Microphone"
                  placeholder="Default microphone"
                  value={config?.preferred_mic ?? DEFAULT_MIC_ID}
                  options={micOptions}
                  triggerClassName={getFocusClassName(NAV.general.microphone)}
                  onValueChange={(value) =>
                    mutate({ preferred_mic: value === DEFAULT_MIC_ID ? null : value })
                  }
                />
              </Field>

              <Field>
                <Label>Mic monitor gain</Label>
                <Hint>
                  Volume of your microphone played back through the speakers while monitoring (
                  {micMonitorGainPct}%)
                </Hint>
                <Slider
                  min={0}
                  max={200}
                  step={1}
                  value={[micMonitorGainPct]}
                  onValueChange={([pct]) => updateMicMonitorGain(pct / 100)}
                  className={getFocusClassName(NAV.general.micMonitorGain)}
                />
              </Field>

              <MicLatencyField
                selectedMicId={config?.preferred_mic ?? null}
                latencySec={micLatencySec}
                sliderClassName={getFocusClassName(NAV.general.micLatency, 0)}
                buttonClassName={getFocusClassName(NAV.general.micLatency, 1)}
                onLatencyChange={updateMicLatency}
              />

              <Field>
                <Label htmlFor="lyrics-vertical-position-1">Lyrics vertical position</Label>
                <Hint>Top moves playback HUD and pitch graph to the bottom</Hint>
                <SettingsSelect
                  id="lyrics-vertical-position-1"
                  label="Lyrics vertical position"
                  placeholder="Select vertical position"
                  value={config?.lyrics_vertical_position ?? DEFAULTS.lyrics_vertical_position}
                  options={LYRICS_VERTICAL_POSITIONS}
                  triggerClassName={getFocusClassName(NAV.general.lyricsVerticalPosition)}
                  onValueChange={(lyrics_vertical_position) =>
                    mutate({
                      lyrics_vertical_position:
                        lyrics_vertical_position as AppConfig["lyrics_vertical_position"],
                    })
                  }
                />
              </Field>

              <Field>
                <Label htmlFor="lyrics-horizontal-position-1">Lyrics horizontal position</Label>
                <Hint>Align lyrics left, center, or right during playback</Hint>
                <SettingsSelect
                  id="lyrics-horizontal-position-1"
                  label="Lyrics horizontal position"
                  placeholder="Select horizontal position"
                  value={config?.lyrics_horizontal_position ?? DEFAULTS.lyrics_horizontal_position}
                  options={LYRICS_HORIZONTAL_POSITIONS}
                  triggerClassName={getFocusClassName(NAV.general.lyricsHorizontalPosition)}
                  onValueChange={(lyrics_horizontal_position) =>
                    mutate({
                      lyrics_horizontal_position:
                        lyrics_horizontal_position as AppConfig["lyrics_horizontal_position"],
                    })
                  }
                />
              </Field>

              <Field>
                <Label>Rotate background videos</Label>
                <Hint>
                  Cut to a different Pixabay clip each time the current one ends, instead of looping
                  a single clip for the whole song
                </Hint>
                <ButtonGroup>
                  <Button
                    variant={config?.pixabay_video_rotation === true ? "outline" : "default"}
                    onClick={() => mutate({ pixabay_video_rotation: false })}
                    className={getFocusClassName(NAV.general.pixabayRotation, 0)}
                  >
                    Off
                  </Button>
                  <Button
                    variant={config?.pixabay_video_rotation === true ? "default" : "outline"}
                    onClick={() => mutate({ pixabay_video_rotation: true })}
                    className={getFocusClassName(NAV.general.pixabayRotation, 1)}
                  >
                    On
                  </Button>
                </ButtonGroup>
              </Field>

              {showBackgroundVideos && (
                <Field>
                  <Label>Karaoke video backgrounds</Label>
                  <Hint>
                    Download up to 240 Pixabay clips per category and stitch them into looping reels
                    used as the background for rendered karaoke videos. Both run in the background
                    and can take several minutes.
                  </Hint>
                  <div className="flex flex-col gap-4">
                    {BACKGROUND_VIDEO_FLAVORS.map(({ value: flavor, label }, flavorIndex) => {
                      const count = videoCounts[flavor];
                      const atVideoCap = count !== undefined && count.count >= count.cap;
                      const reelCount = reelCounts[flavor];
                      const atReelCap = reelCount !== undefined && reelCount.count >= reelCount.cap;

                      return (
                        <div key={flavor} className="flex flex-col gap-1.5">
                          <div className="flex items-baseline gap-2">
                            <span className="text-sm font-medium">{label}</span>
                            {count && (
                              <span className="text-xs text-muted-foreground">
                                {count.count} / {count.cap} cached
                              </span>
                            )}
                            {reelCount && (
                              <span className="text-xs text-muted-foreground">
                                {reelCount.count} / {reelCount.cap} reels
                              </span>
                            )}
                          </div>
                          <ButtonGroup>
                            <Button
                              type="button"
                              variant="outline"
                              disabled={pixabayDownloadStatus[flavor] === "running" || atVideoCap}
                              onClick={() => startPixabayDownload(flavor)}
                              className={getFocusClassName(
                                NAV.general.backgroundVideos,
                                flavorIndex * 2,
                              )}
                            >
                              {pixabayDownloadStatus[flavor] === "running" && (
                                <Loader2Icon className="size-4 animate-spin" />
                              )}
                              {atVideoCap ? "Cap reached" : "Download videos"}
                            </Button>
                            <Button
                              type="button"
                              variant="outline"
                              disabled={reelBuildStatus[flavor] === "running"}
                              onClick={() => startReelBuild(flavor)}
                              className={getFocusClassName(
                                NAV.general.backgroundVideos,
                                flavorIndex * 2 + 1,
                              )}
                            >
                              {reelBuildStatus[flavor] === "running" && (
                                <Loader2Icon className="size-4 animate-spin" />
                              )}
                              {atReelCap ? "Regenerate reels" : "Build reels"}
                            </Button>
                          </ButtonGroup>
                          {pixabayDownloadMessage[flavor] && (
                            <p className="text-sm text-muted-foreground">
                              {pixabayDownloadMessage[flavor]}
                            </p>
                          )}
                          {reelBuildMessage[flavor] && (
                            <p className="text-sm text-muted-foreground">
                              {reelBuildMessage[flavor]}
                            </p>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </Field>
              )}
            </FieldGroup>
          </TabsContent>

          <TabsContent value="analysis" className="mt-4">
            <FieldGroup>
              <Field>
                <Label htmlFor="separator-1">Vocal separator</Label>
                <Hint>How vocals are split from the music.</Hint>
                <SettingsSelect
                  id="separator-1"
                  label="Separator"
                  placeholder="Select a separator"
                  value={config?.separator ?? DEFAULTS.separator}
                  options={SEPARATORS}
                  triggerClassName={getFocusClassName(analysisNav.separator)}
                  onValueChange={(separator) => mutate({ separator })}
                />
              </Field>

              <Field>
                <Label htmlFor="asr-engine-1">Transcription model</Label>
                <Hint>Turns the vocals into lyrics.</Hint>
                <SettingsSelect
                  id="asr-engine-1"
                  label="ASR Engine"
                  placeholder="Select an engine"
                  value={asrEngine}
                  options={ASR_ENGINES}
                  triggerClassName={getFocusClassName(analysisNav.asrEngine)}
                  onValueChange={(asr_engine) => mutate({ asr_engine })}
                />
              </Field>

              {!isParakeet && (
                <>
                  <Field>
                    <Label htmlFor="model-1">Model size</Label>
                    <Hint>Smaller models are faster but produce worse results</Hint>
                    <SettingsSelect
                      id="model-1"
                      label="Model size"
                      placeholder="Select a model size"
                      value={config?.whisper_model ?? DEFAULTS.whisper_model}
                      options={modelOptions}
                      triggerClassName={getFocusClassName(analysisNav.whisperModel)}
                      onValueChange={(whisper_model) => mutate({ whisper_model })}
                    />
                  </Field>

                  <Field>
                    <Label>Beam Size</Label>
                    <Hint>Higher values improve accuracy at the cost of speed</Hint>
                    <NumberButtonGroup
                      name="beam_size"
                      value={beamSize}
                      segment={analysisNav.beamSize}
                      getFocusClassName={getFocusClassName}
                      onChange={(beam_size) => mutate({ beam_size })}
                    />
                  </Field>
                </>
              )}

              <Field>
                <Label htmlFor="align-backend-1">Alignment model</Label>
                <Hint>How each word is timed to the audio.</Hint>
                <SettingsSelect
                  id="align-backend-1"
                  label="Forced alignment"
                  placeholder="Select an alignment backend"
                  value={config?.align_backend ?? DEFAULTS.align_backend}
                  options={ALIGN_BACKENDS}
                  triggerClassName={getFocusClassName(analysisNav.alignBackend)}
                  onValueChange={(align_backend) => mutate({ align_backend })}
                />
              </Field>

              <Field>
                <Label>Auto-analyze</Label>
                <Hint>Automatically queue every unanalyzed song after scans finish</Hint>
                <ButtonGroup>
                  <Button
                    variant={config?.auto_analyze === true ? "outline" : "default"}
                    onClick={() => mutate({ auto_analyze: false })}
                    className={getFocusClassName(analysisNav.autoAnalyze, 0)}
                  >
                    Off
                  </Button>
                  <Button
                    variant={config?.auto_analyze === true ? "default" : "outline"}
                    onClick={() => mutate({ auto_analyze: true })}
                    className={getFocusClassName(analysisNav.autoAnalyze, 1)}
                  >
                    On
                  </Button>
                </ButtonGroup>
              </Field>

              <Field>
                <Label>Timing diagnostics</Label>
                <Hint>
                  Log how long each analysis stage takes and record it in a local table, tagged with
                  the settings used, for troubleshooting slow analyses
                </Hint>
                <ButtonGroup>
                  <Button
                    variant={config?.track_analysis_timings === false ? "default" : "outline"}
                    onClick={() => mutate({ track_analysis_timings: false })}
                    className={getFocusClassName(analysisNav.trackTimings, 0)}
                  >
                    Off
                  </Button>
                  <Button
                    variant={config?.track_analysis_timings === false ? "outline" : "default"}
                    onClick={() => mutate({ track_analysis_timings: true })}
                    className={getFocusClassName(analysisNav.trackTimings, 1)}
                  >
                    On
                  </Button>
                </ButtonGroup>
              </Field>

              <Field>
                <Label>Vocal detection sensitivity</Label>
                <Hint>
                  How loud the vocals must be to count as the song's start and end. Lower it if
                  quiet intros, outros, or soft singing get cut off; raise it to trim more silence (
                  {vocalThresholdDisplayPct}% of the loudest moment)
                </Hint>
                <Slider
                  min={0}
                  max={Math.round(VOCAL_THRESHOLD_MAX * 100)}
                  step={1}
                  value={[vocalThresholdDisplayPct]}
                  onValueChange={([pct]) => updateVocalThreshold(pct / 100)}
                  className={getFocusClassName(analysisNav.vocalThreshold)}
                />
              </Field>

              <Field>
                <Label>Batch Size</Label>
                <Hint>Higher values use more memory but process faster</Hint>
                <NumberButtonGroup
                  name="batch_size"
                  value={batchSize}
                  segment={analysisNav.batchSize}
                  getFocusClassName={getFocusClassName}
                  onChange={(batch_size) => mutate({ batch_size })}
                />
              </Field>

              <Field>
                <Label>Use external lyrics</Label>
                <Hint>
                  When a song has a .lrc file or embedded lyrics tag (.lrc preferred), align that
                  text to the vocals instead of transcribing it
                </Hint>
                <ButtonGroup>
                  <Button
                    variant={config?.use_external_lyrics === true ? "outline" : "default"}
                    onClick={() => mutate({ use_external_lyrics: false })}
                    className={getFocusClassName(analysisNav.useExternalLyrics, 0)}
                  >
                    Off
                  </Button>
                  <Button
                    variant={config?.use_external_lyrics === true ? "default" : "outline"}
                    onClick={() => mutate({ use_external_lyrics: true })}
                    className={getFocusClassName(analysisNav.useExternalLyrics, 1)}
                  >
                    On
                  </Button>
                </ButtonGroup>
              </Field>

              <Field>
                <Label>Restore analysis queue</Label>
                <Hint>
                  Re-queue songs that were still queued, analyzing, or failed when the server last
                  stopped, instead of clearing the queue on startup
                </Hint>
                <ButtonGroup>
                  <Button
                    variant={config?.restore_analyze === true ? "outline" : "default"}
                    onClick={() => mutate({ restore_analyze: false })}
                    className={getFocusClassName(analysisNav.restoreAnalyze, 0)}
                  >
                    Off
                  </Button>
                  <Button
                    variant={config?.restore_analyze === true ? "default" : "outline"}
                    onClick={() => mutate({ restore_analyze: true })}
                    className={getFocusClassName(analysisNav.restoreAnalyze, 1)}
                  >
                    On
                  </Button>
                </ButtonGroup>
              </Field>

              <Field>
                <Label htmlFor="align-backend-2">Alternative alignment model</Label>
                <Hint>
                  Backend used only by a song's "Realign (alternative backend)" action, so you can
                  try a different one without changing the default above.
                </Hint>
                <SettingsSelect
                  id="align-backend-2"
                  label="Alternative forced alignment"
                  placeholder="Select an alternative alignment backend"
                  value={config?.alt_align_backend ?? DEFAULTS.alt_align_backend}
                  options={ALIGN_BACKENDS}
                  triggerClassName={getFocusClassName(analysisNav.altAlignBackend)}
                  onValueChange={(alt_align_backend) => mutate({ alt_align_backend })}
                />
              </Field>

              {showParallelAnalysis && (
                <>
                  <Field>
                    <Label>Parallel analysis</Label>
                    <Hint>
                      Offload queued songs to another Nightingale server instead of only analyzing
                      them here. Songs are only sent over once confirmed present (same file and
                      path) on the peer.
                    </Hint>
                    <ButtonGroup>
                      <Button
                        variant={config?.parallel_analysis_enabled === true ? "outline" : "default"}
                        onClick={() => mutate({ parallel_analysis_enabled: false })}
                        className={getFocusClassName(analysisNav.parallelAnalysisEnabled, 0)}
                      >
                        Off
                      </Button>
                      <Button
                        variant={config?.parallel_analysis_enabled === true ? "default" : "outline"}
                        onClick={() => mutate({ parallel_analysis_enabled: true })}
                        className={getFocusClassName(analysisNav.parallelAnalysisEnabled, 1)}
                      >
                        On
                      </Button>
                    </ButtonGroup>
                  </Field>

                  <Field>
                    <Label htmlFor="parallel-analysis-url-1">Peer server address</Label>
                    <Hint>
                      Base URL of the other Nightingale instance, e.g. http://otherhost:8080
                    </Hint>
                    <div className="flex gap-2">
                      <Input
                        id="parallel-analysis-url-1"
                        placeholder="http://otherhost:8080"
                        value={parallelUrl}
                        onChange={(event) => {
                          setParallelUrl(event.target.value);
                          setPingStatus("idle");
                        }}
                        onBlur={commitParallelUrl}
                        className={getFocusClassName(analysisNav.parallelAnalysisUrl)}
                      />
                      <Button
                        type="button"
                        variant="outline"
                        onClick={pingParallel}
                        className={getFocusClassName(analysisNav.parallelAnalysisPing)}
                      >
                        {pingStatus === "loading" ? (
                          <Loader2Icon className="size-4 animate-spin" />
                        ) : pingStatus === "alive" ? (
                          <CheckCircle2Icon className="size-4 text-chart-3" />
                        ) : pingStatus === "unreachable" ? (
                          <XCircleIcon className="size-4 text-destructive" />
                        ) : null}
                        Ping
                      </Button>
                    </div>
                    {pingStatus === "alive" && (
                      <p className="text-sm text-muted-foreground">Peer is reachable.</p>
                    )}
                    {pingStatus === "unreachable" && (
                      <p className="text-sm text-destructive">Peer did not respond.</p>
                    )}
                  </Field>

                  <Field>
                    <Label>Parallel analysis only</Label>
                    <Hint>
                      Never analyze songs on this instance -- only the peer above processes the
                      queue. A song the peer rejects or times out on stays queued for the peer to
                      retry instead of falling back to local analysis.
                    </Hint>
                    <ButtonGroup>
                      <Button
                        variant={config?.parallel_analysis_only === true ? "outline" : "default"}
                        onClick={() => mutate({ parallel_analysis_only: false })}
                        className={getFocusClassName(analysisNav.parallelAnalysisOnly, 0)}
                      >
                        Off
                      </Button>
                      <Button
                        variant={config?.parallel_analysis_only === true ? "default" : "outline"}
                        onClick={() => mutate({ parallel_analysis_only: true })}
                        className={getFocusClassName(analysisNav.parallelAnalysisOnly, 1)}
                      >
                        On
                      </Button>
                    </ButtonGroup>
                  </Field>
                </>
              )}
            </FieldGroup>
          </TabsContent>
        </Tabs>

        <div className="flex flex-col-reverse gap-2 border-t pt-4 sm:flex-row sm:justify-end">
          <Button
            variant="ghost"
            onClick={resetDefaults}
            className={getFocusClassName(footerSegment, 0)}
          >
            Restore Defaults
          </Button>
          <Button variant="outline" onClick={close} className={getFocusClassName(footerSegment, 1)}>
            Close
          </Button>
        </div>
      </div>
    </div>
  );
};
