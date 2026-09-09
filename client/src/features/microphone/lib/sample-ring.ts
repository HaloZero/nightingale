/**
 * Fixed-capacity ring buffer of `f32` mono samples. Cheap to feed (`push`)
 * from the mic-frame stream, and lets DSP consumers grab a contiguous window
 * of the most recent samples without per-frame allocation.
 */
export class SampleRing {
  private readonly buffer: Float32Array;
  private writeIdx = 0;
  private filled = 0;

  constructor(public readonly capacity: number) {
    this.buffer = new Float32Array(capacity);
  }

  push(samples: ArrayLike<number>): void {
    const cap = this.capacity;
    for (let i = 0; i < samples.length; i++) {
      this.buffer[this.writeIdx] = samples[i];
      this.writeIdx = (this.writeIdx + 1) % cap;
    }
    if (this.filled < cap) {
      this.filled = Math.min(cap, this.filled + samples.length);
    }
  }

  /** Number of valid samples currently stored (caps at capacity). */
  get size(): number {
    return this.filled;
  }

  /**
   * Copies the most recent `out.length` samples (oldest first) into `out`.
   * Returns `false` if not enough samples have been buffered yet.
   */
  readMostRecent(out: Float32Array): boolean {
    const n = out.length;
    if (n > this.capacity || this.filled < n) {
      return false;
    }
    const cap = this.capacity;
    const start = (this.writeIdx - n + cap) % cap;
    const tail = cap - start;
    if (tail >= n) {
      out.set(this.buffer.subarray(start, start + n), 0);
    } else {
      out.set(this.buffer.subarray(start), 0);
      out.set(this.buffer.subarray(0, n - tail), tail);
    }
    return true;
  }

  reset(): void {
    this.writeIdx = 0;
    this.filled = 0;
    this.buffer.fill(0);
  }
}
