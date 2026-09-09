import { CheckCircle2Icon, Loader2Icon, XCircleIcon } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router';

import { pingParallelAnalysis } from '@/bridge/analysis';
import {
  buildBackgroundReels,
  type CountAndCap,
  downloadAllPixabayVideos,
  getBackgroundReelCount,
  getBackgroundVideoCount,
  onBackgroundReelsDone,
  onBackgroundReelsProgress,
  onPixabayBulkDownloadDone,
  onPixabayBulkDownloadProgress,
} from '@/bridge/background-videos';
import { setFullScreen, isFullScreen as tauriIsFullScreen } from '@/bridge/fullScreen';
import { isTauri } from '@/bridge/runtime';
import { clampPlaybackScale } from '@/features/playback/lib/display-scale';
import {
  ALIGN_BACKENDS,
  ASR_ENGINES,
  BACKGROUND_VIDEO_FLAVORS,
  DEFAULTS,
  LYRICS_HORIZONTAL_POSITIONS,
  LYRICS_VERTICAL_POSITIONS,
  MODELS,
  NAV,
  PLAYBACK_MODES,
  PLAYBACK_SCALE_MAX,
  PLAYBACK_SCALE_MIN,
  PLAYBACK_SCALE_STEP,
  SEPARATORS,
  SETTINGS_TABS,
  VOCAL_THRESHOLD_MAX,
  getAnalysisNav,
  type SettingsTab,
} from '@/features/settings/components/constants';
import { MicrophoneSettings } from '@/features/settings/components/microphone-settings';
import { PlaybackPreview } from '@/features/settings/components/playback-preview';
import {
  Hint,
  NumberButtonGroup,
  PageHeader,
  SettingsSelect,
} from '@/features/settings/components/settings-controls';
import { useSettingsNavigation } from '@/features/settings/hooks/use-settings-navigation';
import { Button } from '@/shared/components/ui/button';
import { ButtonGroup } from '@/shared/components/ui/button-group';
import { Field, FieldGroup } from '@/shared/components/ui/field';
import { Input } from '@/shared/components/ui/input';
import { Label } from '@/shared/components/ui/label';
import { Slider } from '@/shared/components/ui/slider';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/shared/components/ui/tabs';
import { useConfig } from '@/shared/config/use-config';
import { useConfigMutation } from '@/shared/config/use-config-mutation';
import type { AppConfig } from '@/types/AppConfig';

const generalSettings = (config: AppConfig | undefined) => {
  if (!config) {
    return {
      fullscreen: undefined,
      preferredMic: null,
      micMonitorGain: DEFAULTS.mic_monitor_gain,
      micLatency: DEFAULTS.mic_latency_compensation_sec,
    };
  }

  return {
    fullscreen: config.fullscreen,
    preferredMic: config.preferred_mic,
    micMonitorGain: config.mic_monitor_gain ?? DEFAULTS.mic_monitor_gain,
    micLatency: config.mic_latency_compensation_sec ?? DEFAULTS.mic_latency_compensation_sec,
  };
};

const playbackSettings = (config: AppConfig | undefined) => ({
  mode: config?.playback_mode ?? DEFAULTS.playback_mode,
  lyricsVertical: config?.lyrics_vertical_position ?? DEFAULTS.lyrics_vertical_position,
  lyricsHorizontal: config?.lyrics_horizontal_position ?? DEFAULTS.lyrics_horizontal_position,
  lyricsScale: clampPlaybackScale(config?.lyrics_scale),
  pitchGraphScale: clampPlaybackScale(config?.pitch_graph_scale),
});

const pendingValue = <T,>(input: T | null, saved: T): T => input ?? saved;

const analysisSettings = (config: AppConfig | undefined) => {
  if (!config) {
    return {
      asrEngine: DEFAULTS.asr_engine,
      separator: DEFAULTS.separator,
      whisperModel: DEFAULTS.whisper_model,
      beamSize: DEFAULTS.beam_size,
      alignBackend: DEFAULTS.align_backend,
      altAlignBackend: DEFAULTS.alt_align_backend,
      autoAnalyze: DEFAULTS.auto_analyze,
      batchSize: DEFAULTS.batch_size,
      vocalThreshold: DEFAULTS.vocal_detection_threshold_pct,
      useExternalLyrics: DEFAULTS.use_external_lyrics,
      restoreAnalyze: DEFAULTS.restore_analyze,
    };
  }

  return {
    asrEngine: config.asr_engine ?? DEFAULTS.asr_engine,
    separator: config.separator ?? DEFAULTS.separator,
    whisperModel: config.whisper_model ?? DEFAULTS.whisper_model,
    beamSize: config.beam_size ?? DEFAULTS.beam_size,
    alignBackend: config.align_backend ?? DEFAULTS.align_backend,
    altAlignBackend: config.alt_align_backend ?? DEFAULTS.alt_align_backend,
    autoAnalyze: config.auto_analyze === true,
    batchSize: config.batch_size ?? DEFAULTS.batch_size,
    vocalThreshold: config.vocal_detection_threshold_pct ?? DEFAULTS.vocal_detection_threshold_pct,
    useExternalLyrics: config.use_external_lyrics === true,
    restoreAnalyze: config.restore_analyze === true,
  };
};

const isSettingsTab = (value: string): value is SettingsTab =>
  SETTINGS_TABS.some((tab) => tab.value === value);

type BackgroundVideoFlavorRowProps = {
  flavor: string;
  label: string;
  flavorIndex: number;
  count: CountAndCap | undefined;
  reelCount: CountAndCap | undefined;
  downloadStatus: 'idle' | 'running' | 'done' | undefined;
  downloadMessage: string | undefined;
  reelStatus: 'idle' | 'running' | 'done' | undefined;
  reelMessage: string | undefined;
  getFocusClassName: (segment: number, slot?: number) => string;
  onStartDownload: (flavor: string) => void;
  onStartReelBuild: (flavor: string) => void;
};

function atCap(count: CountAndCap | undefined): boolean {
  return count !== undefined && count.count >= count.cap;
}

function BackgroundVideoFlavorRow({
  flavor,
  label,
  flavorIndex,
  count,
  reelCount,
  downloadStatus,
  downloadMessage,
  reelStatus,
  reelMessage,
  getFocusClassName,
  onStartDownload,
  onStartReelBuild,
}: BackgroundVideoFlavorRowProps) {
  const atVideoCap = atCap(count);
  const atReelCap = atCap(reelCount);

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-baseline gap-2">
        <span className="text-sm font-medium">{label}</span>
        {count !== undefined && (
          <span className="text-xs text-muted-foreground">
            {count.count} / {count.cap} cached
          </span>
        )}
        {reelCount !== undefined && (
          <span className="text-xs text-muted-foreground">
            {reelCount.count} / {reelCount.cap} reels
          </span>
        )}
      </div>
      <ButtonGroup>
        <Button
          type="button"
          variant="outline"
          disabled={downloadStatus === 'running' || atVideoCap}
          onClick={() => onStartDownload(flavor)}
          className={getFocusClassName(NAV.general.backgroundVideos, flavorIndex * 2)}
        >
          {downloadStatus === 'running' && <Loader2Icon className="size-4 animate-spin" />}
          {atVideoCap ? 'Cap reached' : 'Download videos'}
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={reelStatus === 'running'}
          onClick={() => onStartReelBuild(flavor)}
          className={getFocusClassName(NAV.general.backgroundVideos, flavorIndex * 2 + 1)}
        >
          {reelStatus === 'running' && <Loader2Icon className="size-4 animate-spin" />}
          {atReelCap ? 'Regenerate reels' : 'Build reels'}
        </Button>
      </ButtonGroup>
      {downloadMessage !== undefined && (
        <p className="text-sm text-muted-foreground">{downloadMessage}</p>
      )}
      {reelMessage !== undefined && <p className="text-sm text-muted-foreground">{reelMessage}</p>}
    </div>
  );
}

type PingStatus = 'idle' | 'loading' | 'alive' | 'unreachable';

function PingStatusIcon({ status }: { status: PingStatus }) {
  if (status === 'loading') {
    return <Loader2Icon className="size-4 animate-spin" />;
  }
  if (status === 'alive') {
    return <CheckCircle2Icon className="size-4 text-chart-3" />;
  }
  if (status === 'unreachable') {
    return <XCircleIcon className="size-4 text-destructive" />;
  }
  return null;
}

type BackgroundVideoStatusMap = Partial<Record<string, 'idle' | 'running' | 'done'>>;
type BackgroundVideoMessageMap = Partial<Record<string, string>>;
type BackgroundVideoCountMap = Partial<Record<string, CountAndCap>>;

type GeneralTabProps = {
  isFullScreen: boolean | null | undefined;
  preferredMic: string | null;
  micMonitorGain: number;
  micLatencySec: number;
  getFocusClassName: (segment: number, slot?: number) => string;
  onToggleWindowMode: (fullscreen: boolean) => void;
  onMicMonitorGainChange: (gain: number) => void;
  onMicLatencyChange: (latencySec: number) => void;
  showBackgroundVideos: boolean;
  videoCounts: BackgroundVideoCountMap;
  reelCounts: BackgroundVideoCountMap;
  pixabayDownloadStatus: BackgroundVideoStatusMap;
  pixabayDownloadMessage: BackgroundVideoMessageMap;
  reelBuildStatus: BackgroundVideoStatusMap;
  reelBuildMessage: BackgroundVideoMessageMap;
  onStartDownload: (flavor: string) => void;
  onStartReelBuild: (flavor: string) => void;
};

function GeneralTab({
  isFullScreen,
  preferredMic,
  micMonitorGain,
  micLatencySec,
  getFocusClassName,
  onToggleWindowMode,
  onMicMonitorGainChange,
  onMicLatencyChange,
  showBackgroundVideos,
  videoCounts,
  reelCounts,
  pixabayDownloadStatus,
  pixabayDownloadMessage,
  reelBuildStatus,
  reelBuildMessage,
  onStartDownload,
  onStartReelBuild,
}: GeneralTabProps) {
  return (
    <TabsContent value="general" className="mt-4">
      <FieldGroup>
        <Field>
          <Label>Window</Label>
          <ButtonGroup>
            <Button
              variant={isFullScreen === true ? 'outline' : 'default'}
              onClick={() => onToggleWindowMode(false)}
              className={getFocusClassName(NAV.general.window, 0)}
            >
              Windowed
            </Button>
            <Button
              variant={isFullScreen === false ? 'outline' : 'default'}
              onClick={() => onToggleWindowMode(true)}
              className={getFocusClassName(NAV.general.window, 1)}
            >
              Fullscreen
            </Button>
          </ButtonGroup>
        </Field>

        <MicrophoneSettings
          savedMicId={preferredMic}
          monitorGain={micMonitorGain}
          latencySec={micLatencySec}
          getFocusClassName={getFocusClassName}
          onMonitorGainChange={onMicMonitorGainChange}
          onLatencyChange={onMicLatencyChange}
        />

        {showBackgroundVideos && (
          <Field>
            <Label>Karaoke video backgrounds</Label>
            <Hint>
              Download up to 240 Pixabay clips per category and stitch them into looping reels used
              as the background for rendered karaoke videos. Both run in the background and can take
              several minutes.
            </Hint>
            <div className="flex flex-col gap-4">
              {BACKGROUND_VIDEO_FLAVORS.map(({ value: flavor, label }, flavorIndex) => (
                <BackgroundVideoFlavorRow
                  key={flavor}
                  flavor={flavor}
                  label={label}
                  flavorIndex={flavorIndex}
                  count={videoCounts[flavor]}
                  reelCount={reelCounts[flavor]}
                  downloadStatus={pixabayDownloadStatus[flavor]}
                  downloadMessage={pixabayDownloadMessage[flavor]}
                  reelStatus={reelBuildStatus[flavor]}
                  reelMessage={reelBuildMessage[flavor]}
                  getFocusClassName={getFocusClassName}
                  onStartDownload={onStartDownload}
                  onStartReelBuild={onStartReelBuild}
                />
              ))}
            </div>
          </Field>
        )}
      </FieldGroup>
    </TabsContent>
  );
}

type ParallelAnalysisSectionProps = {
  analysisNav: ReturnType<typeof getAnalysisNav>;
  getFocusClassName: (segment: number, slot?: number) => string;
  parallelAnalysisEnabled: boolean;
  parallelAnalysisOnly: boolean;
  parallelUrl: string;
  pingStatus: PingStatus;
  onMutate: (partialConfig: Partial<AppConfig>) => void;
  onParallelUrlChange: (value: string) => void;
  onCommitParallelUrl: () => void;
  onPingParallel: () => Promise<void>;
};

function ParallelAnalysisSection({
  analysisNav,
  getFocusClassName,
  parallelAnalysisEnabled,
  parallelAnalysisOnly,
  parallelUrl,
  pingStatus,
  onMutate,
  onParallelUrlChange,
  onCommitParallelUrl,
  onPingParallel,
}: ParallelAnalysisSectionProps) {
  return (
    <>
      <Field>
        <Label>Parallel analysis</Label>
        <Hint>
          Offload queued songs to another Nightingale server instead of only analyzing them here.
          Songs are only sent over once confirmed present (same file and path) on the peer.
        </Hint>
        <ButtonGroup>
          <Button
            variant={parallelAnalysisEnabled ? 'outline' : 'default'}
            onClick={() => onMutate({ parallel_analysis_enabled: false })}
            className={getFocusClassName(analysisNav.parallelAnalysisEnabled, 0)}
          >
            Off
          </Button>
          <Button
            variant={parallelAnalysisEnabled ? 'default' : 'outline'}
            onClick={() => onMutate({ parallel_analysis_enabled: true })}
            className={getFocusClassName(analysisNav.parallelAnalysisEnabled, 1)}
          >
            On
          </Button>
        </ButtonGroup>
      </Field>

      <Field>
        <Label htmlFor="parallel-analysis-url-1">Peer server address</Label>
        <Hint>Base URL of the other Nightingale instance, e.g. http://otherhost:8080</Hint>
        <div className="flex gap-2">
          <Input
            id="parallel-analysis-url-1"
            placeholder="http://otherhost:8080"
            value={parallelUrl}
            onChange={(event) => onParallelUrlChange(event.target.value)}
            onBlur={onCommitParallelUrl}
            className={getFocusClassName(analysisNav.parallelAnalysisUrl)}
          />
          <Button
            type="button"
            variant="outline"
            onClick={() => void onPingParallel()}
            className={getFocusClassName(analysisNav.parallelAnalysisPing)}
          >
            <PingStatusIcon status={pingStatus} />
            Ping
          </Button>
        </div>
        {pingStatus === 'alive' && (
          <p className="text-sm text-muted-foreground">Peer is reachable.</p>
        )}
        {pingStatus === 'unreachable' && (
          <p className="text-sm text-destructive">Peer did not respond.</p>
        )}
      </Field>

      <Field>
        <Label>Parallel analysis only</Label>
        <Hint>
          Never analyze songs on this instance -- only the peer above processes the queue. A song
          the peer rejects or times out on stays queued for the peer to retry instead of falling
          back to local analysis.
        </Hint>
        <ButtonGroup>
          <Button
            variant={parallelAnalysisOnly ? 'outline' : 'default'}
            onClick={() => onMutate({ parallel_analysis_only: false })}
            className={getFocusClassName(analysisNav.parallelAnalysisOnly, 0)}
          >
            Off
          </Button>
          <Button
            variant={parallelAnalysisOnly ? 'default' : 'outline'}
            onClick={() => onMutate({ parallel_analysis_only: true })}
            className={getFocusClassName(analysisNav.parallelAnalysisOnly, 1)}
          >
            On
          </Button>
        </ButtonGroup>
      </Field>
    </>
  );
}

type AnalysisTabProps = {
  analysis: ReturnType<typeof analysisSettings>;
  asrEngine: string;
  isParakeet: boolean;
  modelOptions: { value: string; label: string }[];
  vocalThresholdDisplayPct: number;
  analysisNav: ReturnType<typeof getAnalysisNav>;
  getFocusClassName: (segment: number, slot?: number) => string;
  onMutate: (partialConfig: Partial<AppConfig>) => void;
  onVocalThresholdChange: (pct: number) => void;
  showParallelAnalysis: boolean;
  parallelAnalysisEnabled: boolean;
  parallelAnalysisOnly: boolean;
  parallelUrl: string;
  pingStatus: PingStatus;
  onParallelUrlChange: (value: string) => void;
  onCommitParallelUrl: () => void;
  onPingParallel: () => Promise<void>;
};

function AnalysisTab({
  analysis,
  asrEngine,
  isParakeet,
  modelOptions,
  vocalThresholdDisplayPct,
  analysisNav,
  getFocusClassName,
  onMutate,
  onVocalThresholdChange,
  showParallelAnalysis,
  parallelAnalysisEnabled,
  parallelAnalysisOnly,
  parallelUrl,
  pingStatus,
  onParallelUrlChange,
  onCommitParallelUrl,
  onPingParallel,
}: AnalysisTabProps) {
  return (
    <TabsContent value="analysis" className="mt-4">
      <FieldGroup>
        <Field>
          <Label htmlFor="separator-1">Vocal separator</Label>
          <Hint>How vocals are split from the music.</Hint>
          <SettingsSelect
            id="separator-1"
            label="Separator"
            placeholder="Select a separator"
            value={analysis.separator}
            options={SEPARATORS}
            triggerClassName={getFocusClassName(analysisNav.separator)}
            onValueChange={(separator) => onMutate({ separator })}
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
            onValueChange={(asr_engine) => onMutate({ asr_engine })}
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
                value={analysis.whisperModel}
                options={modelOptions}
                triggerClassName={getFocusClassName(analysisNav.whisperModel)}
                onValueChange={(whisper_model) => onMutate({ whisper_model })}
              />
            </Field>

            <Field>
              <Label>Beam Size</Label>
              <Hint>Higher values improve accuracy at the cost of speed</Hint>
              <NumberButtonGroup
                name="beam_size"
                value={analysis.beamSize}
                segment={analysisNav.beamSize}
                getFocusClassName={getFocusClassName}
                onChange={(beam_size) => onMutate({ beam_size })}
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
            value={analysis.alignBackend}
            options={ALIGN_BACKENDS}
            triggerClassName={getFocusClassName(analysisNav.alignBackend)}
            onValueChange={(align_backend) => onMutate({ align_backend })}
          />
        </Field>

        <Field>
          <Label>Auto-analyze</Label>
          <Hint>Automatically queue every unanalyzed song after scans finish</Hint>
          <ButtonGroup>
            <Button
              variant={analysis.autoAnalyze ? 'outline' : 'default'}
              onClick={() => onMutate({ auto_analyze: false })}
              className={getFocusClassName(analysisNav.autoAnalyze, 0)}
            >
              Off
            </Button>
            <Button
              variant={analysis.autoAnalyze ? 'default' : 'outline'}
              onClick={() => onMutate({ auto_analyze: true })}
              className={getFocusClassName(analysisNav.autoAnalyze, 1)}
            >
              On
            </Button>
          </ButtonGroup>
        </Field>

        <Field>
          <Label>Vocal detection sensitivity</Label>
          <Hint>
            How loud the vocals must be to count as the song's start and end. Lower it if quiet
            intros, outros, or soft singing get cut off; raise it to trim more silence (
            {vocalThresholdDisplayPct}% of the loudest moment)
          </Hint>
          <Slider
            min={0}
            max={Math.round(VOCAL_THRESHOLD_MAX * 100)}
            step={1}
            value={[vocalThresholdDisplayPct]}
            onValueChange={([pct]) => onVocalThresholdChange(pct / 100)}
            className={getFocusClassName(analysisNav.vocalThreshold)}
          />
        </Field>

        <Field>
          <Label>Batch Size</Label>
          <Hint>Higher values use more memory but process faster</Hint>
          <NumberButtonGroup
            name="batch_size"
            value={analysis.batchSize}
            segment={analysisNav.batchSize}
            getFocusClassName={getFocusClassName}
            onChange={(batch_size) => onMutate({ batch_size })}
          />
        </Field>

        <Field>
          <Label>Use external lyrics</Label>
          <Hint>
            When a song has a .lrc file or embedded lyrics tag (.lrc preferred), align that text to
            the vocals instead of transcribing it
          </Hint>
          <ButtonGroup>
            <Button
              variant={analysis.useExternalLyrics ? 'outline' : 'default'}
              onClick={() => onMutate({ use_external_lyrics: false })}
              className={getFocusClassName(analysisNav.useExternalLyrics, 0)}
            >
              Off
            </Button>
            <Button
              variant={analysis.useExternalLyrics ? 'default' : 'outline'}
              onClick={() => onMutate({ use_external_lyrics: true })}
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
              variant={analysis.restoreAnalyze ? 'outline' : 'default'}
              onClick={() => onMutate({ restore_analyze: false })}
              className={getFocusClassName(analysisNav.restoreAnalyze, 0)}
            >
              Off
            </Button>
            <Button
              variant={analysis.restoreAnalyze ? 'default' : 'outline'}
              onClick={() => onMutate({ restore_analyze: true })}
              className={getFocusClassName(analysisNav.restoreAnalyze, 1)}
            >
              On
            </Button>
          </ButtonGroup>
        </Field>

        <Field>
          <Label htmlFor="align-backend-2">Alternative alignment model</Label>
          <Hint>
            Backend used only by a song's "Realign (alternative backend)" action, so you can try a
            different one without changing the default above.
          </Hint>
          <SettingsSelect
            id="align-backend-2"
            label="Alternative forced alignment"
            placeholder="Select an alternative alignment backend"
            value={analysis.altAlignBackend}
            options={ALIGN_BACKENDS}
            triggerClassName={getFocusClassName(analysisNav.altAlignBackend)}
            onValueChange={(alt_align_backend) => onMutate({ alt_align_backend })}
          />
        </Field>

        {showParallelAnalysis && (
          <ParallelAnalysisSection
            analysisNav={analysisNav}
            getFocusClassName={getFocusClassName}
            parallelAnalysisEnabled={parallelAnalysisEnabled}
            parallelAnalysisOnly={parallelAnalysisOnly}
            parallelUrl={parallelUrl}
            pingStatus={pingStatus}
            onMutate={onMutate}
            onParallelUrlChange={onParallelUrlChange}
            onCommitParallelUrl={onCommitParallelUrl}
            onPingParallel={onPingParallel}
          />
        )}
      </FieldGroup>
    </TabsContent>
  );
}

export const SettingsPage = () => {
  const navigate = useNavigate();
  const { data: config } = useConfig();
  const { mutate } = useConfigMutation();

  const containerRef = useRef<HTMLDivElement>(null);
  const [tab, setTab] = useState<SettingsTab>('general');
  const general = generalSettings(config);
  const playback = playbackSettings(config);
  const analysis = analysisSettings(config);
  const [isFullScreen, setIsFullScreen] = useState<boolean | null | undefined>(general.fullscreen);
  const [micMonitorGainInput, setMicMonitorGain] = useState<number | null>(null);
  const micMonitorGain = micMonitorGainInput ?? general.micMonitorGain;
  const [micLatencySecInput, setMicLatencySec] = useState<number | null>(null);
  const micLatencySec = micLatencySecInput ?? general.micLatency;
  const [lyricsVerticalInput, setLyricsVertical] = useState<string | null>(null);
  const lyricsVertical = pendingValue(lyricsVerticalInput, playback.lyricsVertical);
  const [lyricsHorizontalInput, setLyricsHorizontal] = useState<string | null>(null);
  const lyricsHorizontal = pendingValue(lyricsHorizontalInput, playback.lyricsHorizontal);
  const [lyricsScaleInput, setLyricsScale] = useState<number | null>(null);
  const lyricsScale = pendingValue(lyricsScaleInput, playback.lyricsScale);
  const [pitchGraphScaleInput, setPitchGraphScale] = useState<number | null>(null);
  const pitchGraphScale = pendingValue(pitchGraphScaleInput, playback.pitchGraphScale);
  const [vocalThresholdPctInput, setVocalThresholdPct] = useState<number | null>(null);
  const vocalThresholdPct = vocalThresholdPctInput ?? analysis.vocalThreshold;
  const [parallelUrlInput, setParallelUrlInput] = useState<string | null>(null);
  const savedParallelUrl = config?.parallel_analysis_url ?? DEFAULTS.parallel_analysis_url;
  const parallelUrl = pendingValue(parallelUrlInput, savedParallelUrl);
  const [pingStatus, setPingStatus] = useState<'idle' | 'loading' | 'alive' | 'unreachable'>(
    'idle',
  );
  // Keyed by flavor (`BACKGROUND_VIDEO_FLAVORS`) -- each flavor's download
  // and reel build run independently, so their status/progress can't share
  // a single value the way the rest of this page's flat state does.
  const [pixabayDownloadStatus, setPixabayDownloadStatus] = useState<
    Partial<Record<string, 'idle' | 'running' | 'done'>>
  >({});
  const [pixabayDownloadMessage, setPixabayDownloadMessage] = useState<
    Partial<Record<string, string>>
  >({});
  const [reelBuildStatus, setReelBuildStatus] = useState<
    Partial<Record<string, 'idle' | 'running' | 'done'>>
  >({});
  const [reelBuildMessage, setReelBuildMessage] = useState<Partial<Record<string, string>>>({});
  const [videoCounts, setVideoCounts] = useState<Partial<Record<string, CountAndCap>>>({});
  const [reelCounts, setReelCounts] = useState<Partial<Record<string, CountAndCap>>>({});

  const close = (): void => {
    void navigate('/');
  };
  const asrEngine = analysis.asrEngine;
  const isParakeet = asrEngine === 'parakeet';
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

  const modelOptions = useMemo(() => MODELS.map((model) => ({ value: model, label: model })), []);
  const lyricsScalePct = Math.round(lyricsScale * 100);
  const pitchGraphScalePct = Math.round(pitchGraphScale * 100);
  const vocalThresholdDisplayPct = Math.round(vocalThresholdPct * 100);

  useEffect(() => {
    const updateIsFullScreen = async () => {
      setIsFullScreen(await tauriIsFullScreen());
    };

    void updateIsFullScreen();
  }, []);

  const refreshVideoCount = useCallback((flavor: string) => {
    void getBackgroundVideoCount(flavor).then((result) => {
      setVideoCounts((prev) => ({ ...prev, [flavor]: result }));
      return undefined;
    });
  }, []);

  const refreshReelCount = useCallback((flavor: string) => {
    void getBackgroundReelCount(flavor).then((result) => {
      setReelCounts((prev) => ({ ...prev, [flavor]: result }));
      return undefined;
    });
  }, []);

  useEffect(() => {
    if (!showBackgroundVideos) {
      return;
    }

    for (const { value: flavor } of BACKGROUND_VIDEO_FLAVORS) {
      refreshVideoCount(flavor);
      refreshReelCount(flavor);
    }
  }, [showBackgroundVideos, refreshVideoCount, refreshReelCount]);

  useEffect(() => {
    if (!showBackgroundVideos) {
      return undefined;
    }

    let unlistenDownloadProgress: (() => void) | undefined;
    let unlistenDownloadDone: (() => void) | undefined;
    let unlistenReelsProgress: (() => void) | undefined;
    let unlistenReelsDone: (() => void) | undefined;

    void onPixabayBulkDownloadProgress(({ flavor, message }) => {
      setPixabayDownloadMessage((prev) => ({ ...prev, [flavor]: message }));
      return undefined;
    }).then((fn) => {
      unlistenDownloadProgress = fn;
      return undefined;
    });
    void onPixabayBulkDownloadDone(({ flavor }) => {
      setPixabayDownloadStatus((prev) => ({ ...prev, [flavor]: 'done' }));
      setPixabayDownloadMessage((prev) => ({ ...prev, [flavor]: 'Download complete.' }));
      refreshVideoCount(flavor);
      return undefined;
    }).then((fn) => {
      unlistenDownloadDone = fn;
      return undefined;
    });
    void onBackgroundReelsProgress(({ flavor, message }) => {
      setReelBuildMessage((prev) => ({ ...prev, [flavor]: message }));
      return undefined;
    }).then((fn) => {
      unlistenReelsProgress = fn;
      return undefined;
    });
    void onBackgroundReelsDone(({ flavor }) => {
      setReelBuildStatus((prev) => ({ ...prev, [flavor]: 'done' }));
      setReelBuildMessage((prev) => ({ ...prev, [flavor]: 'Reels built.' }));
      refreshReelCount(flavor);
      return undefined;
    }).then((fn) => {
      unlistenReelsDone = fn;
      return undefined;
    });

    return () => {
      unlistenDownloadProgress?.();
      unlistenDownloadDone?.();
      unlistenReelsProgress?.();
      unlistenReelsDone?.();
    };
  }, [showBackgroundVideos, refreshVideoCount, refreshReelCount]);

  const updateMicMonitorGain = (gain: number) => {
    setMicMonitorGain(gain);
    mutate({ mic_monitor_gain: gain });
  };

  const updateMicLatency = (latencySec: number) => {
    setMicLatencySec(latencySec);
    mutate({ mic_latency_compensation_sec: latencySec });
  };

  const updateLyricsScale = (scale: number) => {
    setLyricsScale(scale);
    mutate({ lyrics_scale: scale });
  };

  const updatePitchGraphScale = (scale: number) => {
    setPitchGraphScale(scale);
    mutate({ pitch_graph_scale: scale });
  };

  const updateVocalThreshold = (pct: number) => {
    setVocalThresholdPct(pct);
    mutate({ vocal_detection_threshold_pct: pct });
  };

  const toggleWindowMode = (fullscreen: boolean) => {
    setIsFullScreen(fullscreen);
    void setFullScreen(fullscreen);
    mutate({ fullscreen });
  };

  const commitParallelUrl = () => {
    const trimmed = parallelUrl.trim();
    setParallelUrlInput(trimmed);
    if (trimmed !== (config?.parallel_analysis_url ?? '')) {
      mutate({ parallel_analysis_url: trimmed === '' ? null : trimmed });
    }
  };

  const pingParallel = async () => {
    const url = parallelUrl.trim();
    if (pingStatus === 'loading' || url.length === 0) {
      return;
    }
    // Also persist whatever's being tested -- if you're pinging it, you
    // want it saved, and it means the field never silently drifts from
    // what Ping last confirmed reachable.
    commitParallelUrl();
    setPingStatus('loading');
    try {
      const alive = await pingParallelAnalysis(url);
      setPingStatus(alive ? 'alive' : 'unreachable');
    } catch {
      setPingStatus('unreachable');
    }
  };

  const startPixabayDownload = (flavor: string) => {
    if (pixabayDownloadStatus[flavor] === 'running') {
      return;
    }
    setPixabayDownloadStatus((prev) => ({ ...prev, [flavor]: 'running' }));
    setPixabayDownloadMessage((prev) => ({ ...prev, [flavor]: 'Starting download...' }));
    void downloadAllPixabayVideos(flavor);
  };

  const startReelBuild = (flavor: string) => {
    if (reelBuildStatus[flavor] === 'running') {
      return;
    }
    setReelBuildStatus((prev) => ({ ...prev, [flavor]: 'running' }));
    setReelBuildMessage((prev) => ({ ...prev, [flavor]: 'Starting reel build...' }));
    void buildBackgroundReels(flavor);
  };

  const resetDefaults = () => {
    mutate(DEFAULTS);
    setMicMonitorGain(DEFAULTS.mic_monitor_gain);
    setMicLatencySec(DEFAULTS.mic_latency_compensation_sec);
    setLyricsVertical(DEFAULTS.lyrics_vertical_position);
    setLyricsHorizontal(DEFAULTS.lyrics_horizontal_position);
    setLyricsScale(DEFAULTS.lyrics_scale);
    setPitchGraphScale(DEFAULTS.pitch_graph_scale);
    setVocalThresholdPct(DEFAULTS.vocal_detection_threshold_pct);
    setParallelUrlInput(DEFAULTS.parallel_analysis_url);
    setPingStatus('idle');
  };

  const { footerSegment, getFocusClassName, syncFocusFromElement } = useSettingsNavigation({
    containerRef,
    tab,
    isParakeet,
    showParallelAnalysis,
    showBackgroundVideos,
    micMonitorGain,
    micLatencySec,
    lyricsScale,
    pitchGraphScale,
    vocalThresholdPct,
    onBack: close,
    onTabChange: setTab,
    onMicMonitorGainChange: updateMicMonitorGain,
    onMicLatencyChange: updateMicLatency,
    onLyricsScaleChange: updateLyricsScale,
    onPitchGraphScaleChange: updatePitchGraphScale,
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

        <Tabs
          value={tab}
          onValueChange={(value) => {
            if (isSettingsTab(value)) {
              setTab(value);
            }
          }}
        >
          <TabsList className="scrollbar-hide max-w-full overflow-x-auto overflow-y-hidden sm:w-fit">
            {SETTINGS_TABS.map((settingsTab, slot) => (
              <TabsTrigger
                key={settingsTab.value}
                value={settingsTab.value}
                className={getFocusClassName(NAV.tabSegment, slot)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') {
                    setTab(settingsTab.value);
                  }
                }}
              >
                {settingsTab.label}
              </TabsTrigger>
            ))}
          </TabsList>

          <GeneralTab
            isFullScreen={isFullScreen}
            preferredMic={general.preferredMic}
            micMonitorGain={micMonitorGain}
            micLatencySec={micLatencySec}
            getFocusClassName={getFocusClassName}
            onToggleWindowMode={toggleWindowMode}
            onMicMonitorGainChange={updateMicMonitorGain}
            onMicLatencyChange={updateMicLatency}
            showBackgroundVideos={showBackgroundVideos}
            videoCounts={videoCounts}
            reelCounts={reelCounts}
            pixabayDownloadStatus={pixabayDownloadStatus}
            pixabayDownloadMessage={pixabayDownloadMessage}
            reelBuildStatus={reelBuildStatus}
            reelBuildMessage={reelBuildMessage}
            onStartDownload={startPixabayDownload}
            onStartReelBuild={startReelBuild}
          />

          <TabsContent value="playback" className="mt-4">
            <div className="space-y-5">
              <div className="w-[65%]">
                <PlaybackPreview
                  lyricsVerticalPosition={lyricsVertical}
                  lyricsHorizontalPosition={lyricsHorizontal}
                  lyricsScale={lyricsScale}
                  pitchGraphScale={pitchGraphScale}
                />
              </div>

              <FieldGroup>
                <Field>
                  <Label htmlFor="playback-mode-1">Playback mode</Label>
                  <Hint>Choose whether playback replaces the menu or runs beside it</Hint>
                  <SettingsSelect
                    id="playback-mode-1"
                    label="Playback mode"
                    placeholder="Select playback mode"
                    value={playback.mode}
                    options={PLAYBACK_MODES}
                    triggerClassName={getFocusClassName(NAV.playback.mode)}
                    onValueChange={(playback_mode) => mutate({ playback_mode })}
                  />
                </Field>

                <Field>
                  <Label htmlFor="lyrics-vertical-position-1">Lyrics vertical position</Label>
                  <Hint>Top moves playback HUD and pitch graph to the bottom</Hint>
                  <SettingsSelect
                    id="lyrics-vertical-position-1"
                    label="Lyrics vertical position"
                    placeholder="Select vertical position"
                    value={lyricsVertical}
                    options={LYRICS_VERTICAL_POSITIONS}
                    triggerClassName={getFocusClassName(NAV.playback.lyricsVerticalPosition)}
                    onValueChange={(lyrics_vertical_position) => {
                      setLyricsVertical(lyrics_vertical_position);
                      mutate({ lyrics_vertical_position });
                    }}
                  />
                </Field>

                <Field>
                  <Label htmlFor="lyrics-horizontal-position-1">Lyrics horizontal position</Label>
                  <Hint>Align lyrics left, center, or right during playback</Hint>
                  <SettingsSelect
                    id="lyrics-horizontal-position-1"
                    label="Lyrics horizontal position"
                    placeholder="Select horizontal position"
                    value={lyricsHorizontal}
                    options={LYRICS_HORIZONTAL_POSITIONS}
                    triggerClassName={getFocusClassName(NAV.playback.lyricsHorizontalPosition)}
                    onValueChange={(lyrics_horizontal_position) => {
                      setLyricsHorizontal(lyrics_horizontal_position);
                      mutate({ lyrics_horizontal_position });
                    }}
                  />
                </Field>

                <Field>
                  <Label>Lyrics scale</Label>
                  <Hint>Size of lyrics during playback ({lyricsScalePct}%)</Hint>
                  <Slider
                    min={PLAYBACK_SCALE_MIN * 100}
                    max={PLAYBACK_SCALE_MAX * 100}
                    step={PLAYBACK_SCALE_STEP * 100}
                    value={[lyricsScalePct]}
                    onValueChange={([pct]) => updateLyricsScale(pct / 100)}
                    className={getFocusClassName(NAV.playback.lyricsScale)}
                  />
                </Field>

                <Field>
                  <Label>Pitch graph scale</Label>
                  <Hint>Size of pitch graph during playback ({pitchGraphScalePct}%)</Hint>
                  <Slider
                    min={PLAYBACK_SCALE_MIN * 100}
                    max={PLAYBACK_SCALE_MAX * 100}
                    step={PLAYBACK_SCALE_STEP * 100}
                    value={[pitchGraphScalePct]}
                    onValueChange={([pct]) => updatePitchGraphScale(pct / 100)}
                    className={getFocusClassName(NAV.playback.pitchGraphScale)}
                  />
                </Field>
              </FieldGroup>
            </div>
          </TabsContent>

          <AnalysisTab
            analysis={analysis}
            asrEngine={asrEngine}
            isParakeet={isParakeet}
            modelOptions={modelOptions}
            vocalThresholdDisplayPct={vocalThresholdDisplayPct}
            analysisNav={analysisNav}
            getFocusClassName={getFocusClassName}
            onMutate={mutate}
            onVocalThresholdChange={updateVocalThreshold}
            showParallelAnalysis={showParallelAnalysis}
            parallelAnalysisEnabled={config?.parallel_analysis_enabled === true}
            parallelAnalysisOnly={config?.parallel_analysis_only === true}
            parallelUrl={parallelUrl}
            pingStatus={pingStatus}
            onParallelUrlChange={(value) => {
              setParallelUrlInput(value);
              setPingStatus('idle');
            }}
            onCommitParallelUrl={commitParallelUrl}
            onPingParallel={pingParallel}
          />
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
