import { TrophyIcon } from 'lucide-react';
import { Fragment, useEffect } from 'react';
import { toast } from 'sonner';

import {
  bestKaraokeVideo,
  forceBestKaraokeVideo,
  onKaraokeVideoReady,
} from '@/bridge/karaoke-video';
import { isTauri } from '@/bridge/runtime';
import { useAnalysis } from '@/features/library/hooks/use-analysis';
import { useDialog } from '@/features/menu/hooks/use-dialog';
import { useProfiles } from '@/features/profiles/queries/use-profiles';
import { Separator } from '@/shared/components/ui/separator';
import type { Song } from '@/types/Song';

import type { SongStatusInfo } from '../shared/song-status';
import { ActionItem } from './action-item';
import { buildActionGroups } from './song-actions';

type ActionsSectionProps = {
  song: Song;
  status: SongStatusInfo;
  analysisBusy: boolean;
  supportsAnalysisActions: boolean;
};

const run =
  (
    message: string,
    action: () => void | boolean | undefined | Promise<void | boolean | undefined>,
    onFalse?: string,
  ) =>
  async (): Promise<void> => {
    const result = await action();
    toast.info(
      result === false && typeof onFalse === 'string' && onFalse !== '' ? onFalse : message,
    );
  };

export const ActionsSection = ({
  song,
  status,
  analysisBusy,
  supportsAnalysisActions,
}: ActionsSectionProps) => {
  const { setMode } = useDialog();
  const analysis = useAnalysis();
  const { data: profiles } = useProfiles();
  const hasScores = profiles?.scores.some((score) => score.song_hash === song.file_hash) ?? false;

  // Unlike the analysis actions above (no completion signal, so `run` just
  // acks the dispatch), rendering has a real completion event -- worth
  // reporting the actual outcome instead of only acking that it started.
  // `music_video_found: false` means the render fell back to a reel
  // background rather than failing outright, so it gets its own toast tone
  // distinct from a genuine failure.
  useEffect(() => {
    if (isTauri) {
      return undefined;
    }

    let unlisten: (() => void) | undefined;

    void onKaraokeVideoReady((event) => {
      if (event.file_hash !== song.file_hash) {
        return;
      }
      if (event.error !== null) {
        toast.error(`Karaoke video failed for "${song.title}": ${event.error}`);
      } else if (!event.music_video_found) {
        toast.success(
          `Karaoke video ready for "${song.title}" (no official music video found, used a background reel)`,
        );
      } else {
        toast.success(`Karaoke video ready for "${song.title}"`);
      }
    }).then((fn) => {
      unlisten = fn;
      return undefined;
    });

    return () => unlisten?.();
  }, [song.file_hash, song.title]);

  const groups = buildActionGroups({
    song,
    status,
    analysisBusy,
    supportsAnalysisActions,
    analysis,
    onEditLyrics: () => setMode({ mode: 'edit-lyrics', song }),
    onChangeLanguage: () => setMode({ mode: 'language', song }),
    run,
    showKaraokeVideoActions: !isTauri,
    onRenderKaraokeVideo: () => {
      toast.info(`Rendering karaoke video for "${song.title}"...`);
      void bestKaraokeVideo(song.file_hash);
    },
    onForceRerenderKaraokeVideo: () => {
      toast.info(`Re-rendering karaoke video for "${song.title}"...`);
      void forceBestKaraokeVideo(song.file_hash);
    },
  });

  if (hasScores) {
    groups.unshift([
      {
        icon: TrophyIcon,
        title: 'Leaderboard',
        description: 'View the best score from each profile.',
        onClick: () => setMode({ mode: 'song-leaderboard', song }),
      },
    ]);
  }

  return (
    <section className="px-2 py-4" aria-labelledby="song-actions-heading">
      <h3 id="song-actions-heading" className="mb-2 px-2 text-xs font-semibold">
        Actions
      </h3>
      <div className="flex flex-col gap-1">
        {groups.map((group, groupIndex) => (
          <Fragment key={group.map((item) => item.title).join('|')}>
            {groupIndex > 0 ? <Separator className="my-1" /> : null}
            {group.map((item) => (
              <ActionItem key={item.title} {...item} />
            ))}
          </Fragment>
        ))}
      </div>
    </section>
  );
};
