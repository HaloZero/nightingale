import type { VideoFlavor } from './video-flavor';

function shuffled<T>(items: readonly T[]): T[] {
  const copy = [...items];

  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }

  return copy;
}

/**
 * In-memory store of Pixabay video URLs per flavor.
 *
 * - The "pool" is the canonical, deduped set of URLs known for a flavor.
 * - The "queue" is a shuffled play order drawn from the pool; once exhausted,
 *   `pullUrl(flavor, allowFallback=true)` falls back to a round-robin over the
 *   pool so playback never stalls while we're waiting for more downloads.
 * - The "ready" set tracks URLs whose <video> element has fired `canplay`,
 *   so the rotator can prefer slots that won't visibly stall.
 */
function pullKnownUrl(queue: string[], known: ReadonlySet<string>): string | null {
  while (queue.length > 0) {
    const next = queue.shift() ?? '';
    if (next !== '' && known.has(next)) {
      return next;
    }
  }

  return null;
}

export class PixabayFlavorLibrary {
  private readonly poolByFlavor = new Map<VideoFlavor, string[]>();
  private readonly queueByFlavor = new Map<VideoFlavor, string[]>();
  private readonly fallbackIndexByFlavor = new Map<VideoFlavor, number>();
  private readonly readyUrls = new Set<string>();

  registerUrls(flavor: VideoFlavor, urls: readonly string[]): void {
    if (urls.length === 0) {
      return;
    }

    const existingPool = this.poolByFlavor.get(flavor) ?? [];
    const known = new Set(existingPool);
    const fresh = urls.filter((url) => !known.has(url));
    if (fresh.length === 0) {
      return;
    }

    this.poolByFlavor.set(flavor, [...existingPool, ...fresh]);

    const queue = this.queueByFlavor.get(flavor) ?? [];
    this.queueByFlavor.set(flavor, [...queue, ...shuffled(fresh)]);
  }

  removeUrl(flavor: VideoFlavor, url: string): void {
    const pool = this.poolByFlavor.get(flavor) ?? [];
    this.poolByFlavor.set(
      flavor,
      pool.filter((u) => u !== url),
    );

    const queue = this.queueByFlavor.get(flavor) ?? [];
    this.queueByFlavor.set(
      flavor,
      queue.filter((u) => u !== url),
    );

    this.readyUrls.delete(url);
  }

  pullUrl(flavor: VideoFlavor, allowFallback: boolean): string | null {
    const queue = this.queueByFlavor.get(flavor) ?? [];
    const known = new Set(this.poolByFlavor.get(flavor) ?? []);

    const queued = pullKnownUrl(queue, known);
    this.queueByFlavor.set(flavor, queue);
    if (queued !== null) {
      return queued;
    }

    if (!allowFallback) {
      return null;
    }

    const pool = this.poolByFlavor.get(flavor) ?? [];
    if (pool.length === 0) {
      return null;
    }

    const index = this.fallbackIndexByFlavor.get(flavor) ?? 0;
    this.fallbackIndexByFlavor.set(flavor, index + 1);

    return pool[index % pool.length];
  }

  queueLength(flavor: VideoFlavor): number {
    return this.queueByFlavor.get(flavor)?.length ?? 0;
  }

  markReady(url: string): void {
    this.readyUrls.add(url);
  }

  markUnready(url: string): void {
    this.readyUrls.delete(url);
  }

  isReady(url: string): boolean {
    return this.readyUrls.has(url);
  }
}
