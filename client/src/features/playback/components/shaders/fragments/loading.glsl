uniform float uTime;
varying vec2 vUv;

const float WIDESCREEN_RATIO = 16.0 / 9.0;

const float RING_FREQ = 10.0;
const float RING_TIME_SPEED = 2.0;
const float RING_FALLOFF_RADIUS = 2.0;
const float RING_POW = 3.0;

const float SPIRAL_ANGLE_HARMONIC = 4.0;
const float SPIRAL_RADIAL_HARMONIC = 6.0;
const float SPIRAL_TIME_SPEED = 1.5;
const float SPIRAL_FALLOFF_OUTER = 1.8;
const float SPIRAL_FALLOFF_INNER = 0.2;
const float SPIRAL_INTENSITY = 0.5;

const float BLOB_FALLOFF = 3.0;
const float BLOB1_X_FREQ = 0.3;
const float BLOB1_Y_FREQ = 0.4;
const float BLOB1_X_AMP = 0.5;
const float BLOB1_Y_AMP = 0.4;
const float BLOB2_X_FREQ = 0.5;
const float BLOB2_Y_FREQ = 0.3;
const float BLOB2_X_PHASE = 2.0;
const float BLOB2_Y_PHASE = 1.0;
const float BLOB2_X_AMP = 0.4;
const float BLOB2_Y_AMP = 0.5;

const float BREATH_FREQ = 1.2;
const float CENTER_FALLOFF = 3.0;
const float CENTER_BASE = 0.3;
const float CENTER_BREATH_GAIN = 0.3;

const vec3 BG_COLOR = vec3(0.02, 0.01, 0.05);
const vec3 RING_COLOR = vec3(0.06, 0.03, 0.14);
const vec3 SPIRAL_COLOR = vec3(0.04, 0.02, 0.08);
const vec3 BLOB1_COLOR = vec3(0.08, 0.04, 0.18);
const vec3 BLOB2_COLOR = vec3(0.03, 0.07, 0.14);
const vec3 CENTER_COLOR = vec3(0.14, 0.07, 0.25);

const float VIGNETTE_RADIUS = 1.6;
const float VIGNETTE_SOFTNESS = 0.7;
const float OUTPUT_CLAMP_MAX = 0.5;

void main() {
  float t = uTime;
  vec2 uv = vUv * 2.0 - 1.0;
  uv.x *= WIDESCREEN_RATIO;

  float dist = length(uv);
  float angle = atan(uv.y, uv.x);

  float rings = sin(dist * RING_FREQ - t * RING_TIME_SPEED) * 0.5 + 0.5;
  rings *= smoothstep(RING_FALLOFF_RADIUS, 0.0, dist);
  rings = pow(rings, RING_POW);

  float spiral = sin(angle * SPIRAL_ANGLE_HARMONIC + dist * SPIRAL_RADIAL_HARMONIC - t * SPIRAL_TIME_SPEED) * 0.5 + 0.5;
  spiral *= smoothstep(SPIRAL_FALLOFF_OUTER, SPIRAL_FALLOFF_INNER, dist);

  float blob1 = exp(-length(uv - vec2(cos(t * BLOB1_X_FREQ) * BLOB1_X_AMP, sin(t * BLOB1_Y_FREQ) * BLOB1_Y_AMP)) * BLOB_FALLOFF);
  float blob2 = exp(-length(uv - vec2(cos(t * BLOB2_X_FREQ + BLOB2_X_PHASE) * BLOB2_X_AMP, sin(t * BLOB2_Y_FREQ + BLOB2_Y_PHASE) * BLOB2_Y_AMP)) * BLOB_FALLOFF);

  float breath = sin(t * BREATH_FREQ) * 0.5 + 0.5;
  float center = exp(-dist * CENTER_FALLOFF) * (CENTER_BASE + CENTER_BREATH_GAIN * breath);

  vec3 color = BG_COLOR;
  color += RING_COLOR * rings;
  color += SPIRAL_COLOR * spiral * SPIRAL_INTENSITY;
  color += BLOB1_COLOR * blob1;
  color += BLOB2_COLOR * blob2;
  color += CENTER_COLOR * center;

  float vignette = 1.0 - length((vUv - 0.5) * VIGNETTE_RADIUS);
  color *= smoothstep(0.0, VIGNETTE_SOFTNESS, vignette);

  color = clamp(color, vec3(0.0), vec3(OUTPUT_CLAMP_MAX));

  gl_FragColor = vec4(linearToSRGB(color), 1.0);
}
