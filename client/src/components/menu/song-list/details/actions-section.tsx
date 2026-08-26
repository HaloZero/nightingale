import { Separator } from "@/components/ui/separator";
import { useAnalysis } from "@/hooks/use-analysis";
import { useDialog } from "@/hooks/use-dialog";
import type { Song } from "@/types/Song";
import {
  forceRerenderKaraokeVideo,
  onKaraokeVideoReady,
  renderKaraokeVideo,
} from "@/bridge/karaoke-video";
import { isTauri } from "@/bridge/runtime";
import { useProfiles } from "@/queries/use-profiles";
import { TrophyIcon } from "lucide-react";
import { Fragment, useEffect } from "react";
import { toast } from "sonner";
import type { SongStatusInfo } from "../shared/song-status";
import { ActionItem } from "./action-item";
import { buildActionGroups } from "./song-actions";

interface ActionsSectionProps {
  song: Song;
  status: SongStatusInfo;
  analysisBusy: boolean;
  supportsAnalysisActions: boolean;
}

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

  const run = (message: string, action: () => void | Promise<void>) => async () => {
    await action();
    toast.info(message);
  };

  // Unlike the analysis actions above (no completion signal, so `run` just
  // acks the dispatch), rendering has a real completion event -- worth
  // reporting the actual outcome instead of only acking that it started.
  useEffect(() => {
    if (isTauri) return;

    let unlisten: (() => void) | undefined;

    onKaraokeVideoReady((event) => {
      if (event.file_hash !== song.file_hash) return;
      if (event.error) {
        toast.error(`Karaoke video render failed for "${song.title}": ${event.error}`);
      } else {
        toast.success(`Karaoke video ready for "${song.title}"`);
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => unlisten?.();
  }, [song.file_hash, song.title]);

  const groups = buildActionGroups({
    song,
    status,
    analysisBusy,
    supportsAnalysisActions,
    analysis,
    onEditLyrics: () => setMode({ mode: "edit-lyrics", song }),
    onChangeLanguage: () => setMode({ mode: "language", song }),
    run,
    showKaraokeVideoActions: !isTauri,
    onRenderKaraokeVideo: () => {
      toast.info(`Rendering karaoke video for "${song.title}"...`);
      renderKaraokeVideo(song.file_hash);
    },
    onForceRerenderKaraokeVideo: () => {
      toast.info(`Re-rendering karaoke video for "${song.title}"...`);
      forceRerenderKaraokeVideo(song.file_hash);
    },
  });

  if (hasScores) {
    groups.unshift([
      {
        icon: TrophyIcon,
        title: "Leaderboard",
        description: "View the best score from each profile.",
        onClick: () => setMode({ mode: "song-leaderboard", song }),
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
          <Fragment key={groupIndex}>
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
