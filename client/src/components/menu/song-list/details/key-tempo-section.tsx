import type { Song } from "@/types/Song";
import { Shifts } from "../shifts";

interface KeyTempoSectionProps {
  song: Song;
  supportsShifts: boolean;
  tempo: number;
  keyOffset: number;
  disabled?: boolean;
  onTempoChange: (tempo: number) => void;
  onKeyOffsetChange: (keyOffset: number) => void;
}

export const KeyTempoSection = ({
  song,
  supportsShifts,
  tempo,
  keyOffset,
  disabled,
  onTempoChange,
  onKeyOffsetChange,
}: KeyTempoSectionProps) => {
  const sectionClass = supportsShifts ? "px-4 pt-4 pb-2" : "px-4 py-4";

  return (
    <section className={sectionClass} aria-labelledby="song-adjustments-heading">
      <h3 id="song-adjustments-heading" className="mb-2 text-xs font-semibold">
        Key & tempo
      </h3>
      {supportsShifts ? (
        <Shifts
          song={song}
          tempo={tempo}
          keyOffset={keyOffset}
          disabled={disabled}
          onTempoChange={onTempoChange}
          onKeyOffsetChange={onKeyOffsetChange}
        />
      ) : (
        <p className="max-w-72 text-xs leading-relaxed text-muted-foreground">
          Key and tempo controls become available after compatible analysis.
        </p>
      )}
    </section>
  );
};
