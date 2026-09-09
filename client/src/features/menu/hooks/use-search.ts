import { atom, useAtom } from 'jotai';

export const searchAtom = atom('');

export const useSearch = () => {
  const [search, setSearch] = useAtom(searchAtom);

  return {
    search,
    setSearch,
  };
};
