import { songKey } from '@/features/library/lib/song-key';
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from '@/shared/components/ui/empty';
import { Spinner } from '@/shared/components/ui/spinner';
import type { Song } from '@/types/Song';

import { SearchResultRow } from './search-result-row';

type SearchResultsProps = {
  query: string;
  isFetching: boolean;
  results: Song[];
};

export const SearchResults = ({ query, isFetching, results }: SearchResultsProps) => {
  if (query === '') {
    return null;
  }

  if (isFetching) {
    return (
      <div className="flex justify-center py-8">
        <Spinner className="size-6" />
      </div>
    );
  }

  if (results.length === 0) {
    return (
      <Empty>
        <EmptyHeader>
          <EmptyTitle>No results</EmptyTitle>
          <EmptyDescription>No songs match &quot;{query}&quot;.</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  return (
    <ul className="flex flex-col gap-1">
      {results.map((song) => (
        <SearchResultRow key={songKey(song)} song={song} />
      ))}
    </ul>
  );
};
