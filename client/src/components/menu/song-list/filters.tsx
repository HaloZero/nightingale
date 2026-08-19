import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SidebarTrigger } from "@/components/ui/sidebar";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useLibraryFilter } from "@/hooks/use-library-filter";
import { useSearch } from "@/hooks/use-search";
import { Grid2X2Icon, ListIcon } from "lucide-react";
import { useRef } from "react";
import { BulkActionsMenu } from "./bulk-actions-menu";

const DEBOUNCE_MS = 500;
export type SongListView = "table" | "grid";

interface FiltersProps {
  view: SongListView;
  onViewChange: (view: SongListView) => void;
  isSavingView?: boolean;
}

export const Filters = ({ view, onViewChange, isSavingView }: FiltersProps) => {
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const { search, setSearch } = useSearch();
  const { status, transcript_source, setLibraryFilter } = useLibraryFilter();

  const handleChange = (value: string) => {
    clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setSearch(value), DEBOUNCE_MS);
  };

  return (
    <div className="grid w-full grid-cols-2 items-center gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] 2xl:grid-cols-[minmax(12rem,1fr)_auto_auto_auto]">
      <div className="col-span-2 flex min-w-0 items-center gap-2 sm:col-span-3 2xl:col-span-1">
        <SidebarTrigger variant="outline" size="icon" className="shrink-0 md:hidden" />
        <Input
          defaultValue={search}
          onChange={({ target: { value } }) => handleChange(value)}
          className="min-w-0 flex-1"
          placeholder="Search songs"
          aria-label="Search songs"
        />
      </div>
      <Select
        value={status ?? "all"}
        onValueChange={(value) =>
          setLibraryFilter((current) => ({
            ...current,
            status: value === "all" ? null : value,
          }))
        }
      >
        <SelectTrigger aria-label="Filter by analysis status" className="w-full min-w-0 2xl:w-32">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            <SelectLabel>Status</SelectLabel>
            <SelectItem value="all">All statuses</SelectItem>
            <SelectItem value="not_analyzed">Not analyzed</SelectItem>
            <SelectItem value="queued">Queued</SelectItem>
            <SelectItem value="analyzing">Analyzing</SelectItem>
            <SelectItem value="analyzed">Analyzed</SelectItem>
            <SelectItem value="failed">Failed</SelectItem>
          </SelectGroup>
        </SelectContent>
      </Select>
      <Select
        value={transcript_source ?? "all"}
        onValueChange={(value) =>
          setLibraryFilter((current) => ({
            ...current,
            transcript_source: value === "all" ? null : value,
          }))
        }
      >
        <SelectTrigger aria-label="Filter by transcript type" className="w-full min-w-0 2xl:w-32">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            <SelectLabel>Type</SelectLabel>
            <SelectItem value="all">All types</SelectItem>
            <SelectItem value="generated">Generated</SelectItem>
            <SelectItem value="lyrics">AI Aligned</SelectItem>
            <SelectItem value="lrc">LRC</SelectItem>
          </SelectGroup>
        </SelectContent>
      </Select>
      <div className="col-span-2 flex items-center justify-end gap-2 sm:col-span-1">
        <BulkActionsMenu />
        <div
          className="flex shrink-0 rounded-md border bg-input/20 p-0.5"
          role="group"
          aria-label="Song list view"
        >
          <Button
            variant={view === "table" ? "secondary" : "ghost"}
            size="icon-sm"
            disabled={isSavingView}
            onClick={() => onViewChange("table")}
            aria-label="Table view"
            aria-pressed={view === "table"}
            title="Table view"
          >
            <ListIcon />
          </Button>
          <Button
            variant={view === "grid" ? "secondary" : "ghost"}
            size="icon-sm"
            disabled={isSavingView}
            onClick={() => onViewChange("grid")}
            aria-label="Card grid view"
            aria-pressed={view === "grid"}
            title="Card grid view"
          >
            <Grid2X2Icon />
          </Button>
        </div>
      </div>
    </div>
  );
};
