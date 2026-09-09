/**
 * Shader-ready reactive frame. All values are slow-smoothed and ready to
 * pipe straight into shader uniforms. `hue` is a 0..1 phase, `flow` is a
 * radians 0..2pi angle, and `wave` is the downsampled time-domain waveform
 * for the oscilloscope shader.
 *
 * 1:1 port of the former Rust `ReactiveAnalyzer` so desktop and web share a
 * single DSP implementation. Runs on the main thread; for ~16ms cadence and
 * 1024-point FFT the cost is negligible.
 */

export const REACTIVE_FFT_SIZE = 1024;
export const REACTIVE_WAVE_BINS = 256;
export const REACTIVE_EMIT_PERIOD_MS = 16;
const EMIT_DT_SEC = REACTIVE_EMIT_PERIOD_MS / 1000;

const LOW_HZ = 250.0;
const MID_HZ = 2000.0;

const FAST_SMOOTH_ALPHA = 0.35;

const RMS_NORM_GAIN = 6.0;
const BAND_NORM_GAIN = 8.0;

const SLOW_TAU_ENERGY_SEC = 0.3;
const SLOW_TAU_TONE_SEC = 0.7;
const SLOW_TAU_BLEND_SEC = 0.4;

const ENERGY_VOLUME_WEIGHT = 0.7;
const ENERGY_BAND_WEIGHT = 0.3;

const HUE_PERIOD_SEC = 14.0;
const HUE_PITCH_GAIN = 1.2;
const HUE_VOLUME_GAIN = 0.6;

const FLOW_PERIOD_SEC = 30.0;
const FLOW_CENTROID_GAIN = 1.2;
const FLOW_ENERGY_GAIN = 0.7;

const DEFAULT_NEUTRAL = 0.5;

export type MicReactiveEvent = {
  volume: number;
  low: number;
  mid: number;
  high: number;
  centroid: number;
  pitch: number;
  energy: number;
  hue: number;
  flow: number;
  wave: Float32Array;
};

function hannWindow(size: number): Float32Array {
  const w = new Float32Array(size);
  const denom = size - 1;
  for (let i = 0; i < size; i++) {
    const t = i / denom;
    w[i] = 0.5 - 0.5 * Math.cos(2 * Math.PI * t);
  }
  return w;
}

function clamp01(value: number): number {
  if (value < 0) {
    return 0;
  }
  if (value > 1) {
    return 1;
  }
  return value;
}

function lerp(prev: number, next: number, alpha: number): number {
  return prev + alpha * (next - prev);
}

function alphaFromTau(tauSec: number): number {
  return 1 - Math.exp(-EMIT_DT_SEC / Math.max(1e-6, tauSec));
}

/**
 * In-place radix-2 Cooley-Tukey FFT. `real` and `imag` must have length `n`
 * which must be a power of 2. Output replaces the input.
 */
function fftInPlace(real: Float32Array, imag: Float32Array, n: number): void {
  let j = 0;
  for (let i = 1; i < n; i++) {
    let bit = n >> 1;
    while (j & bit) {
      j ^= bit;
      bit >>= 1;
    }
    j ^= bit;
    if (i < j) {
      const tr = real[i];
      real[i] = real[j];
      real[j] = tr;
      const ti = imag[i];
      imag[i] = imag[j];
      imag[j] = ti;
    }
  }

  for (let len = 2; len <= n; len <<= 1) {
    const halfLen = len >> 1;
    const angle = (-2 * Math.PI) / len;
    const wr = Math.cos(angle);
    const wi = Math.sin(angle);
    for (let i = 0; i < n; i += len) {
      let curR = 1;
      let curI = 0;
      for (let k = 0; k < halfLen; k++) {
        const a = i + k;
        const b = a + halfLen;
        const tR = real[b] * curR - imag[b] * curI;
        const tI = real[b] * curI + imag[b] * curR;
        real[b] = real[a] - tR;
        imag[b] = imag[a] - tI;
        real[a] = real[a] + tR;
        imag[a] = imag[a] + tI;
        const nextR = curR * wr - curI * wi;
        const nextI = curR * wi + curI * wr;
        curR = nextR;
        curI = nextI;
      }
    }
  }
}

function downsampleWave(samples: Float32Array, out: Float32Array): void {
  const bins = out.length;
  if (bins === 0) {
    return;
  }
  const chunk = (samples.length / bins) | 0;
  if (chunk === 0) {
    for (let i = 0; i < bins; i++) {
      out[i] = i < samples.length ? samples[i] : 0;
    }
    return;
  }
  for (let b = 0; b < bins; b++) {
    const start = b * chunk;
    const end = b + 1 === bins ? samples.length : start + chunk;
    let sum = 0;
    for (let i = start; i < end; i++) {
      sum += samples[i];
    }
    out[b] = sum / (end - start);
  }
}

type SpectrumStats = {
  lowSum: number;
  midSum: number;
  highSum: number;
  lowCount: number;
  midCount: number;
  highCount: number;
  centroidNum: number;
  centroidDen: number;
  peakBin: number;
  peakMag: number;
};

function prepareFft(
  samples: Float32Array,
  real: Float32Array,
  imag: Float32Array,
  window: Float32Array,
): number {
  let sumSq = 0;
  for (let index = 0; index < REACTIVE_FFT_SIZE; index++) {
    const sample = samples[index];
    sumSq += sample * sample;
    real[index] = sample * window[index];
    imag[index] = 0;
  }
  return Math.sqrt(sumSq / REACTIVE_FFT_SIZE);
}

function analyzeSpectrum(
  real: Float32Array,
  imag: Float32Array,
  sampleRate: number,
): SpectrumStats {
  const stats: SpectrumStats = {
    lowSum: 0,
    midSum: 0,
    highSum: 0,
    lowCount: 0,
    midCount: 0,
    highCount: 0,
    centroidNum: 0,
    centroidDen: 0,
    peakBin: 0,
    peakMag: 0,
  };
  const binHz = sampleRate / REACTIVE_FFT_SIZE;
  const magNorm = 1 / (REACTIVE_FFT_SIZE * 0.5);

  for (let index = 0; index < REACTIVE_FFT_SIZE >> 1; index++) {
    const magnitude = Math.hypot(real[index], imag[index]) * magNorm;
    const frequency = index * binHz;
    if (frequency < LOW_HZ) {
      stats.lowSum += magnitude;
      stats.lowCount += 1;
    } else if (frequency < MID_HZ) {
      stats.midSum += magnitude;
      stats.midCount += 1;
    } else {
      stats.highSum += magnitude;
      stats.highCount += 1;
    }
    stats.centroidNum += frequency * magnitude;
    stats.centroidDen += magnitude;
    if (magnitude > stats.peakMag) {
      stats.peakMag = magnitude;
      stats.peakBin = index;
    }
  }

  return stats;
}

export class ReactiveAnalyzer {
  private readonly window: Float32Array = hannWindow(REACTIVE_FFT_SIZE);
  private readonly real: Float32Array = new Float32Array(REACTIVE_FFT_SIZE);
  private readonly imag: Float32Array = new Float32Array(REACTIVE_FFT_SIZE);
  private readonly waveOut: Float32Array = new Float32Array(REACTIVE_WAVE_BINS);
  private readonly out: MicReactiveEvent;

  private rmsFast = 0;
  private centroidFast = 0;
  private lowFast = 0;
  private midFast = 0;
  private highFast = 0;
  private pitchFast = DEFAULT_NEUTRAL;

  private volumeSlow = 0;
  private lowSlow = 0;
  private midSlow = 0;
  private highSlow = 0;
  private centroidSlow = DEFAULT_NEUTRAL;
  private pitchSlow = DEFAULT_NEUTRAL;
  private energySlow = 0;

  private huePhase = 0;
  private flowAngle = 0;

  constructor(
    private readonly minPitchHz: number,
    private readonly maxPitchHz: number,
    private readonly rmsGate: number,
  ) {
    this.out = {
      volume: 0,
      low: 0,
      mid: 0,
      high: 0,
      centroid: DEFAULT_NEUTRAL,
      pitch: DEFAULT_NEUTRAL,
      energy: 0,
      hue: 0,
      flow: 0,
      wave: this.waveOut,
    };
  }

  /**
   * Consumes a 1024-sample mono window and returns the latest reactive frame.
   * The returned object and its `wave` array are reused; consumers must read
   * before the next `analyze` call or copy.
   */
  analyze(samples: Float32Array, sampleRate: number): MicReactiveEvent {
    if (samples.length !== REACTIVE_FFT_SIZE) {
      throw new Error(`ReactiveAnalyzer expected ${REACTIVE_FFT_SIZE} samples`);
    }

    const rawRms = prepareFft(samples, this.real, this.imag, this.window);
    fftInPlace(this.real, this.imag, REACTIVE_FFT_SIZE);
    const spectrum = analyzeSpectrum(this.real, this.imag, sampleRate);
    const binHz = sampleRate / REACTIVE_FFT_SIZE;
    const nyquist = sampleRate * 0.5;
    const centroidRaw =
      spectrum.centroidDen > 1e-6
        ? clamp01(spectrum.centroidNum / spectrum.centroidDen / nyquist)
        : 0;

    const bandNorm = (sum: number, count: number): number => {
      if (count === 0) {
        return 0;
      }
      return clamp01((sum / count) * BAND_NORM_GAIN);
    };
    const lowRaw = bandNorm(spectrum.lowSum, spectrum.lowCount);
    const midRaw = bandNorm(spectrum.midSum, spectrum.midCount);
    const highRaw = bandNorm(spectrum.highSum, spectrum.highCount);
    const rmsRaw = clamp01(rawRms * RMS_NORM_GAIN);

    this.rmsFast = lerp(this.rmsFast, rmsRaw, FAST_SMOOTH_ALPHA);
    this.centroidFast = lerp(this.centroidFast, centroidRaw, FAST_SMOOTH_ALPHA);
    this.lowFast = lerp(this.lowFast, lowRaw, FAST_SMOOTH_ALPHA);
    this.midFast = lerp(this.midFast, midRaw, FAST_SMOOTH_ALPHA);
    this.highFast = lerp(this.highFast, highRaw, FAST_SMOOTH_ALPHA);

    let pitchNormRaw: number | null = null;
    if (spectrum.peakMag > 0 && rawRms > this.rmsGate) {
      const f = spectrum.peakBin * binHz;
      if (f >= this.minPitchHz && f <= this.maxPitchHz) {
        pitchNormRaw = clamp01((f - this.minPitchHz) / (this.maxPitchHz - this.minPitchHz));
      }
    }
    if (pitchNormRaw !== null) {
      this.pitchFast = lerp(this.pitchFast, pitchNormRaw, FAST_SMOOTH_ALPHA);
    }

    const alphaEnergy = alphaFromTau(SLOW_TAU_ENERGY_SEC);
    const alphaTone = alphaFromTau(SLOW_TAU_TONE_SEC);
    const alphaBlend = alphaFromTau(SLOW_TAU_BLEND_SEC);

    this.volumeSlow = lerp(this.volumeSlow, this.rmsFast, alphaEnergy);
    this.lowSlow = lerp(this.lowSlow, this.lowFast, alphaEnergy);
    this.midSlow = lerp(this.midSlow, this.midFast, alphaEnergy);
    this.highSlow = lerp(this.highSlow, this.highFast, alphaEnergy);
    this.centroidSlow = lerp(this.centroidSlow, this.centroidFast, alphaTone);
    if (pitchNormRaw !== null) {
      this.pitchSlow = lerp(this.pitchSlow, this.pitchFast, alphaTone);
    }

    const energyTarget = clamp01(
      this.volumeSlow * ENERGY_VOLUME_WEIGHT + (this.midSlow + this.highSlow) * ENERGY_BAND_WEIGHT,
    );
    this.energySlow = lerp(this.energySlow, energyTarget, alphaBlend);

    // Phase accumulators are deliberately NOT wrapped — see the Rust comment
    // in the previous implementation. Shaders multiply these by various
    // constants, so wrapping at 1.0 / TAU shows up as a visible jump.
    const huePitchTerm = (this.pitchSlow - DEFAULT_NEUTRAL) * HUE_PITCH_GAIN;
    const hueRate = (1 / HUE_PERIOD_SEC) * (1 + huePitchTerm + this.volumeSlow * HUE_VOLUME_GAIN);
    this.huePhase += hueRate * EMIT_DT_SEC;

    const flowCentroidTerm = (this.centroidSlow - DEFAULT_NEUTRAL) * FLOW_CENTROID_GAIN;
    const flowRate =
      ((2 * Math.PI) / FLOW_PERIOD_SEC) *
      (1 + flowCentroidTerm + this.energySlow * FLOW_ENERGY_GAIN);
    this.flowAngle += flowRate * EMIT_DT_SEC;

    downsampleWave(samples, this.waveOut);

    this.out.volume = this.volumeSlow;
    this.out.low = this.lowSlow;
    this.out.mid = this.midSlow;
    this.out.high = this.highSlow;
    this.out.centroid = this.centroidSlow;
    this.out.pitch = this.pitchSlow;
    this.out.energy = this.energySlow;
    this.out.hue = this.huePhase;
    this.out.flow = this.flowAngle;

    return this.out;
  }
}
