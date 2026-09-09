import loading from './fragments/loading.glsl?raw';
import metaballs from './fragments/metaballs.glsl?raw';
import nebula from './fragments/nebula.glsl?raw';
import oscilloscope from './fragments/oscilloscope.glsl?raw';
import plasma from './fragments/plasma.glsl?raw';
import sonar from './fragments/sonar.glsl?raw';
import spectrum from './fragments/spectrum.glsl?raw';
import starfield from './fragments/starfield.glsl?raw';
import voronoi from './fragments/voronoi.glsl?raw';
import vortex from './fragments/vortex.glsl?raw';
import waves from './fragments/waves.glsl?raw';
import audioUniforms from './utils/audio-uniforms.glsl?raw';
import noise from './utils/noise.glsl?raw';
import srgb from './utils/srgb.glsl?raw';
import vertex from './vertex.glsl?raw';

const compose = (...parts: string[]) => parts.join('\n');
const withAudio = (frag: string) => compose(audioUniforms, srgb, noise, frag);
const standalone = (frag: string) => compose(srgb, frag);

export const vertexShader = vertex;
export const loadingFragment = standalone(loading);

export type ShaderDefinition = {
  name: string;
  fragmentShader: string;
};

export const shaders: ShaderDefinition[] = [
  { name: 'Plasma', fragmentShader: withAudio(plasma) },
  { name: 'Waves', fragmentShader: withAudio(waves) },
  { name: 'Nebula', fragmentShader: withAudio(nebula) },
  { name: 'Starfield', fragmentShader: withAudio(starfield) },
  { name: 'Sonar', fragmentShader: withAudio(sonar) },
  { name: 'Voronoi', fragmentShader: withAudio(voronoi) },
  { name: 'Vortex', fragmentShader: withAudio(vortex) },
  { name: 'Metaballs', fragmentShader: withAudio(metaballs) },
  { name: 'Spectrum', fragmentShader: withAudio(spectrum) },
  { name: 'Oscilloscope', fragmentShader: withAudio(oscilloscope) },
];
