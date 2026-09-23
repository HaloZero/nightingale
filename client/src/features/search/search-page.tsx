import { useQuery } from '@tanstack/react-query';
import { ArrowLeftIcon, SearchIcon } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Link } from 'react-router';

import { searchSongs } from '@/bridge/songs';
import { InputGroup, InputGroupAddon, InputGroupInput } from '@/shared/components/ui/input-group';
import { SEARCH_SONGS } from '@/shared/query-keys';

import { SearchResults } from './components/search-results';

const DEBOUNCE_MS = 300;
const RESULTS_LIMIT = 25;

export const SearchPage = () => {
  const [query, setQuery] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    return () => clearTimeout(timerRef.current);
  }, []);

  const handleChange = (value: string) => {
    setQuery(value);
    clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setDebouncedQuery(value), DEBOUNCE_MS);
  };

  const trimmed = debouncedQuery.trim();

  const { data, isFetching } = useQuery({
    queryKey: [...SEARCH_SONGS, trimmed],
    queryFn: () => searchSongs(trimmed, RESULTS_LIMIT),
    enabled: trimmed !== '',
  });

  return (
    <div className="mx-auto flex min-h-dvh w-full max-w-2xl flex-col gap-6 px-4 pt-[max(3rem,10vh)] pb-8">
      <Link
        to="/"
        className="inline-flex w-fit items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
      >
        <ArrowLeftIcon className="size-4" />
        Back to library
      </Link>

      <InputGroup className="h-14 rounded-xl">
        <InputGroupAddon>
          <SearchIcon className="size-5" />
        </InputGroupAddon>
        <InputGroupInput
          ref={inputRef}
          value={query}
          onChange={({ target: { value } }) => handleChange(value)}
          placeholder="Search songs, artists, albums…"
          aria-label="Search songs"
          className="h-14 text-lg"
        />
      </InputGroup>

      <SearchResults query={trimmed} isFetching={isFetching} results={data ?? []} />
    </div>
  );
};
