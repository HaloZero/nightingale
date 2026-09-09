import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { z } from 'zod';

import { invoke, isTauri, listen, type UnlistenFn } from './runtime';
import { playbackLocationStateSchema } from './schemas';

const SESSION_PLAYBACK_URL = '/playback?session=1';

const playbackSessionSchema = playbackLocationStateSchema.extend({
  queuePlayback: z.boolean(),
});

export type PlaybackSession = z.infer<typeof playbackSessionSchema>;
export type PlaybackTarget = Window | null | undefined;

const parseSession = (value: unknown): PlaybackSession => playbackSessionSchema.parse(value);

export const isSessionPlayback = (): boolean =>
  window.location.pathname === '/playback' &&
  new URLSearchParams(window.location.search).has('session');

export const loadPlaybackSession = async (): Promise<PlaybackSession | null> => {
  const value = await invoke('load_playback_session');
  return value === null ? null : parseSession(value);
};

export const savePlaybackSession = async (session: PlaybackSession): Promise<PlaybackSession> =>
  parseSession(await invoke('save_playback_session', { session }));

export const onPlaybackSessionChanged = async (
  callback: (session: PlaybackSession) => void,
): Promise<UnlistenFn> =>
  await listen<unknown>('playback-session-changed', ({ payload }) =>
    callback(parseSession(payload)),
  );

export const reservePlaybackTarget = (): PlaybackTarget => {
  if (isTauri || isSessionPlayback()) {
    return null;
  }
  return window.open('', 'nightingale-playback') ?? undefined;
};

export const showPlaybackTarget = async (target: PlaybackTarget): Promise<void> => {
  if (!isTauri) {
    if (!target) {
      throw new Error('Browser blocked playback tab');
    }
    if (
      target.location.pathname !== '/playback' ||
      !new URLSearchParams(target.location.search).has('session')
    ) {
      target.location.replace(SESSION_PLAYBACK_URL);
    }
    target.focus();
    return;
  }

  const existing = await WebviewWindow.getByLabel('playback');
  if (existing) {
    await existing.show();
    await existing.setFocus();
    return;
  }

  const playbackWindow = new WebviewWindow('playback', {
    url: SESSION_PLAYBACK_URL,
    title: 'Nightingale Playback',
    width: 1280,
    height: 720,
    decorations: false,
  });
  await new Promise<void>((resolve, reject) => {
    void playbackWindow.once('tauri://created', () => resolve());
    void playbackWindow.once<unknown>('tauri://error', ({ payload }) =>
      reject(new Error(String(payload))),
    );
  });
};
