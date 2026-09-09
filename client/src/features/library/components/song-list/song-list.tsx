import { useEffect, useMemo, useRef, useState, type RefObject } from 'react';

import type { PlaybackQueueEntry } from '@/bridge/playback-queue';
import { useAnalysisQueue, useSongs } from '@/features/library/queries/use-songs';
import { useLibraryFilter } from '@/features/menu/hooks/use-library-filter';
import { useSearch } from '@/features/menu/hooks/use-search';
import { useMenuFocus } from '@/features/menu/providers/menu-focus-context';
import { usePlaybackQueueQuery } from '@/features/playback-queue/use-playback-queue';
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from '@/shared/components/ui/empty';
import { Separator } from '@/shared/components/ui/separator';
import { useConfig } from '@/shared/config/use-config';
import { useConfigMutation } from '@/shared/config/use-config-mutation';
import { useLatestRef } from '@/shared/hooks/use-latest-ref';
import { usePersistentScroll } from '@/shared/hooks/use-persistent-scroll';
import { cn } from '@/shared/utils/cn';
import type { AppConfig } from '@/types/AppConfig';
import type { QueuedStatus } from '@/types/QueuedStatus';
import type { Song } from '@/types/Song';
import type { SongSort } from '@/types/SongSort';
import type { SongSortColumn } from '@/types/SongSortColumn';

import { Filters, type SongListView } from './filters';
import { Progress } from './progress';
import { QueueSidebar } from './queue-sidebar';
import { songKey } from './shared/song-key';
import { SongDetailsSidebar } from './song-details-sidebar';
import type { SongItemProps } from './types';
import { SongGrid } from './views/song-grid';
import { SongTable } from './views/song-table';

type SongCollectionProps = {
  songs: Song[];
  view: SongListView;
  sort: readonly SongSort[];
  sortingDisabled: boolean;
  loading: boolean;
  hasActiveFilter: boolean;
  getItemProps: (song: Song, index: number) => SongItemProps;
  setScrollContainer: (element: HTMLElement | null) => void;
  sentinelRef: RefObject<HTMLDivElement | null>;
  onSort: (column: SongSortColumn) => void;
};

const hasFilters = (values: readonly unknown[]): boolean =>
  values.some((value) => (typeof value === 'string' ? value.trim() !== '' : Boolean(value)));

const songListSort = (config: AppConfig | undefined): readonly SongSort[] =>
  config?.song_list_sort ?? [];

const nextSongSort = (sorts: readonly SongSort[], column: SongSortColumn): SongSort[] | null => {
  const sortIndex = sorts.findIndex((sort) => sort.column === column);
  if (sortIndex === -1) {
    return [...sorts, { column, direction: 'ascending' }];
  }
  if (sorts[sortIndex].direction === 'ascending') {
    return sorts.map((sort, index) =>
      index === sortIndex ? { ...sort, direction: 'descending' } : sort,
    );
  }

  const nextSorts = sorts.filter((_, index) => index !== sortIndex);
  return nextSorts.length === 0 ? null : nextSorts;
};

const EmptySongs = ({ filtered }: { filtered: boolean }) => (
  <Empty className="px-4">
    <EmptyHeader>
      <EmptyTitle>{filtered ? 'No results' : 'No songs found'}</EmptyTitle>
      <EmptyDescription>
        {filtered
          ? 'No songs match your search or filters. Try adjusting them.'
          : 'This library is empty or still being scanned.'}
      </EmptyDescription>
    </EmptyHeader>
  </Empty>
);

const SongCollection = ({
  songs,
  view,
  sort,
  sortingDisabled,
  loading,
  hasActiveFilter,
  getItemProps,
  setScrollContainer,
  sentinelRef,
  onSort,
}: SongCollectionProps) => {
  if (songs.length === 0 && !loading) {
    return <EmptySongs filtered={hasActiveFilter} />;
  }

  return (
    <div
      ref={setScrollContainer}
      data-song-layout={view}
      className="themed-scrollbar song-table-shell min-h-0 max-w-full flex-1 overflow-x-hidden overflow-y-auto pb-[max(0.75rem,env(safe-area-inset-bottom))]"
    >
      {view === 'table' ? (
        <SongTable
          songs={songs}
          sort={sort}
          sortingDisabled={sortingDisabled}
          onSort={onSort}
          getItemProps={getItemProps}
        />
      ) : (
        <SongGrid songs={songs} getItemProps={getItemProps} />
      )}
      <div ref={sentinelRef} className="h-1" aria-hidden="true" />
    </div>
  );
};

type SongSidePanelProps = {
  queueOpen: boolean;
  playbackQueue: PlaybackQueueEntry[];
  song: Song | null;
  queueEntries?: Record<string, QueuedStatus>;
  onCloseQueue: () => void;
  onCloseSong: () => void;
};

function SongSidePanel({
  queueOpen,
  playbackQueue,
  song,
  queueEntries,
  onCloseQueue,
  onCloseSong,
}: SongSidePanelProps) {
  if (queueOpen) {
    return (
      <QueueSidebar
        key={playbackQueue.map(({ id }) => id).join(':')}
        entries={playbackQueue}
        onClose={onCloseQueue}
      />
    );
  }
  if (song) {
    return (
      <SongDetailsSidebar
        key={songKey(song)}
        song={song}
        queueStatus={queueEntries?.[song.file_hash]}
        onClose={onCloseSong}
      />
    );
  }
  return null;
}

export const SongList = () => {
  const { data: queue } = useAnalysisQueue();
  const { data: playbackQueue = [] } = usePlaybackQueueQuery();
  const { data: config } = useConfig();
  const { mutate: saveConfig, isPending: isSavingConfig } = useConfigMutation();
  const { focus, actionsRef, setFocus, selectedSong, setSelectedSong } = useMenuFocus();
  const { setScrollContainer, resetScroll } = usePersistentScroll('songList');
  const { search } = useSearch();
  const { artist, album, playlist, query, status, transcript_source } = useLibraryFilter();
  const { data, fetchNextPage, hasNextPage, isFetchingNextPage, isLoading } = useSongs();
  const [queueOpen, setQueueOpen] = useState(false);
  const view: SongListView = config?.song_list_view === 'grid' ? 'grid' : 'table';
  const sort = songListSort(config);
  const songs = useMemo(() => data?.pages.flatMap((page) => page.processed) ?? [], [data]);
  const selectedKey = selectedSong ? songKey(selectedSong) : null;
  const currentSelectedSong = songs.find((song) => songKey(song) === selectedKey) ?? selectedSong;
  const filterKey = JSON.stringify([
    search,
    artist,
    album,
    playlist,
    query,
    status,
    transcript_source,
    sort,
  ]);
  const previousFilterKeyRef = useRef(filterKey);
  const songsRef = useLatestRef(songs);
  const sentinelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (previousFilterKeyRef.current === filterKey) {
      return;
    }
    previousFilterKeyRef.current = filterKey;

    setSelectedSong(null);
    resetScroll();
    setFocus((previous) => ({ ...previous, songIndex: 0 }));
  }, [filterKey, resetScroll, setFocus, setSelectedSong]);

  useEffect(() => {
    actionsRef.current.songCount = songs.length;
  }, [songs.length, actionsRef]);

  useEffect(() => {
    const actions = actionsRef.current;
    actions.onConfirmSong = (index: number) => {
      const song = songsRef.current.at(index);
      if (!song) {
        return;
      }

      setQueueOpen(false);
      setSelectedSong(song);
    };
    return () => {
      actions.onConfirmSong = null;
    };
  }, [actionsRef, setSelectedSong, songsRef]);

  useEffect(() => {
    const element = sentinelRef.current;
    if (!element) {
      return undefined;
    }

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting && hasNextPage === true && !isFetchingNextPage) {
          void fetchNextPage();
        }
      },
      { rootMargin: '200px' },
    );

    observer.observe(element);
    return () => observer.disconnect();
  }, [fetchNextPage, hasNextPage, isFetchingNextPage]);

  const isSongListActive = focus.active && focus.panel === 'songList';
  const hasActiveFilter = hasFilters([
    search,
    artist,
    album,
    playlist,
    query,
    status,
    transcript_source,
  ]);
  const selectSong = (song: (typeof songs)[number]) => {
    setQueueOpen(false);
    setSelectedSong(song);
  };
  const openQueue = () => {
    setSelectedSong(null);
    setQueueOpen(true);
    setFocus((previous) => ({ ...previous, active: true, panel: 'songDetails' }));
  };
  const sortSongs = (column: SongSortColumn) => {
    saveConfig({ song_list_sort: nextSongSort(sort, column) });
  };

  const getItemProps = (song: (typeof songs)[number], index: number): SongItemProps => ({
    song,
    queueStatus: queue?.entries[song.file_hash],
    index,
    isSelected: selectedKey === songKey(song),
    isFocused: isSongListActive && !focus.actionsFocused && focus.songIndex === index,
    onSelect: () => selectSong(song),
  });

  return (
    <div className="flex min-h-0 w-full flex-1 overflow-hidden">
      <main
        className={cn(
          'min-w-0 flex-1 flex-col gap-3 p-3 sm:p-4',
          currentSelectedSong || queueOpen ? 'hidden xl:flex' : 'flex',
        )}
      >
        <Filters
          view={view}
          queueCount={playbackQueue.length}
          isSavingView={isSavingConfig}
          onOpenQueue={openQueue}
          onViewChange={(nextView) => saveConfig({ song_list_view: nextView })}
        />
        <Separator />
        <Progress />
        <SongCollection
          songs={songs}
          view={view}
          sort={sort}
          sortingDisabled={isSavingConfig}
          loading={isLoading}
          hasActiveFilter={hasActiveFilter}
          getItemProps={getItemProps}
          setScrollContainer={setScrollContainer}
          sentinelRef={sentinelRef}
          onSort={sortSongs}
        />
      </main>

      <SongSidePanel
        queueOpen={queueOpen}
        playbackQueue={playbackQueue}
        song={currentSelectedSong}
        queueEntries={queue?.entries}
        onCloseQueue={() => setQueueOpen(false)}
        onCloseSong={() => setSelectedSong(null)}
      />
    </div>
  );
};
