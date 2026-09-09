import { useFrame } from '@react-three/fiber';
import { useMemo, useRef } from 'react';
import * as THREE from 'three';

import type { MicReactiveRef } from '@/features/microphone/hooks/use-mic-reactive';
import type { MicReactiveEvent } from '@/features/microphone/lib/reactive-analyzer';

const WAVE_BINS = 256;

const WAVE_BYTE_CENTER = 127.5;
const WAVE_BYTE_NEUTRAL = 127;

const TAU_MIC_BLEND_SEC = 0.4;

const NEUTRAL_PITCH = 0.5;
const NEUTRAL_CENTROID = 0.5;

const TIME_FAST_GAIN = 1.4;
const TIME_SLOW_GAIN = 0.65;
const TIME_SLOW_FLOOR = 0.35;

type NumericFieldOf<T> = { [K in keyof T]: T[K] extends number ? K : never }[keyof T];

type ReactiveBinding = {
  uniform: string;
  field: NumericFieldOf<MicReactiveEvent>;
  neutral: number;
};

const REACTIVE_BINDINGS: readonly ReactiveBinding[] = [
  { uniform: 'uVolume', field: 'volume', neutral: 0 },
  { uniform: 'uLow', field: 'low', neutral: 0 },
  { uniform: 'uMid', field: 'mid', neutral: 0 },
  { uniform: 'uHigh', field: 'high', neutral: 0 },
  { uniform: 'uCentroid', field: 'centroid', neutral: NEUTRAL_CENTROID },
  { uniform: 'uPitch', field: 'pitch', neutral: NEUTRAL_PITCH },
  { uniform: 'uEnergy', field: 'energy', neutral: 0 },
  { uniform: 'uHue', field: 'hue', neutral: 0 },
  { uniform: 'uFlow', field: 'flow', neutral: 0 },
];

type Uniform = { value: number | THREE.DataTexture };

export type ShaderUniforms = Record<string, Uniform>;

type AnimationClocks = {
  time: number;
  timeFast: number;
  timeSlow: number;
  reactiveBlend: number;
};

type WaveTexture = {
  texture: THREE.DataTexture;
  sync(wave: ArrayLike<number> | undefined): void;
};

function ema(prev: number, next: number, delta: number, tau: number): number {
  const a = 1 - Math.exp(-delta / Math.max(1e-6, tau));

  return prev + (next - prev) * a;
}

function clampByte(value: number): number {
  if (value < 0) {
    return 0;
  }
  if (value > 255) {
    return 255;
  }

  return value | 0;
}

function createWaveTexture(buffer: Uint8Array): THREE.DataTexture {
  const texture = new THREE.DataTexture(
    buffer,
    buffer.length,
    1,
    THREE.RedFormat,
    THREE.UnsignedByteType,
  );

  texture.magFilter = THREE.LinearFilter;
  texture.minFilter = THREE.LinearFilter;
  texture.wrapS = THREE.ClampToEdgeWrapping;
  texture.wrapT = THREE.ClampToEdgeWrapping;
  texture.needsUpdate = true;

  return texture;
}

function syncWaveTexture(
  buffer: Uint8Array,
  texture: THREE.DataTexture,
  wave: ArrayLike<number> | undefined,
): void {
  if (wave !== undefined && wave.length === buffer.length) {
    for (let i = 0; i < buffer.length; i++) {
      buffer[i] = clampByte(wave[i] * WAVE_BYTE_CENTER + WAVE_BYTE_CENTER);
    }
    texture.needsUpdate = true;
    return;
  }

  let mutated = false;
  for (let i = 0; i < buffer.length; i++) {
    if (buffer[i] !== WAVE_BYTE_NEUTRAL) {
      buffer[i] = WAVE_BYTE_NEUTRAL;
      mutated = true;
    }
  }
  if (mutated) {
    texture.needsUpdate = true;
  }
}

function useWaveTexture(bins: number): WaveTexture {
  const state = useMemo(() => {
    const buffer = new Uint8Array(bins).fill(WAVE_BYTE_NEUTRAL);
    return { buffer, texture: createWaveTexture(buffer) };
  }, [bins]);

  return {
    texture: state.texture,
    sync: (wave) => syncWaveTexture(state.buffer, state.texture, wave),
  };
}

function createInitialUniforms(waveTexture: THREE.DataTexture): ShaderUniforms {
  const uniforms: ShaderUniforms = {
    uTime: { value: 0 },
    uTimeFast: { value: 0 },
    uTimeSlow: { value: 0 },
    uAudioReactive: { value: 0 },
    uWave: { value: waveTexture },
  };

  for (const { uniform, neutral } of REACTIVE_BINDINGS) {
    uniforms[uniform] = { value: neutral };
  }

  return uniforms;
}

function advanceClocks(
  clocks: AnimationClocks,
  reactive: MicReactiveEvent | null,
  delta: number,
  isPlaying: boolean,
): void {
  const target = reactive !== null ? 1 : 0;
  clocks.reactiveBlend = ema(clocks.reactiveBlend, target, delta, TAU_MIC_BLEND_SEC);

  if (!isPlaying) {
    return;
  }

  const audioVol = (reactive?.volume ?? 0) * clocks.reactiveBlend;
  const fastRate = 1 + audioVol * TIME_FAST_GAIN;
  const slowRate = Math.max(TIME_SLOW_FLOOR, 1 - audioVol * TIME_SLOW_GAIN);

  clocks.time += delta;
  clocks.timeFast += delta * fastRate;
  clocks.timeSlow += delta * slowRate;
}

function writeUniforms(
  uniforms: ShaderUniforms,
  reactive: MicReactiveEvent | null,
  clocks: AnimationClocks,
): void {
  uniforms.uTime.value = clocks.time;
  uniforms.uTimeFast.value = clocks.timeFast;
  uniforms.uTimeSlow.value = clocks.timeSlow;
  uniforms.uAudioReactive.value = clocks.reactiveBlend;

  for (const { uniform, field, neutral } of REACTIVE_BINDINGS) {
    uniforms[uniform].value = reactive?.[field] ?? neutral;
  }
}

/**
 * Builds the uniform set consumed by the audio-reactive shaders, and drives
 * its time/audio uniforms each frame from a `MicReactiveRef`.
 *
 * Must be called inside an `<R3F Canvas>` because it relies on `useFrame`.
 */
export function useReactiveShaderUniforms(
  reactiveRef: MicReactiveRef | undefined,
  isPlaying: boolean,
): ShaderUniforms {
  const wave = useWaveTexture(WAVE_BINS);
  const uniforms = useMemo(() => createInitialUniforms(wave.texture), [wave.texture]);
  const clocks = useRef<AnimationClocks>({
    time: 0,
    timeFast: 0,
    timeSlow: 0,
    reactiveBlend: 0,
  });

  useFrame((_, delta) => {
    const reactive = reactiveRef?.current ?? null;

    advanceClocks(clocks.current, reactive, delta, isPlaying);
    writeUniforms(uniforms, reactive, clocks.current);
    wave.sync(reactive?.wave);
  });

  return uniforms;
}
