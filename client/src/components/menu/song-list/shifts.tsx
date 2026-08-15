import type { Song } from "@/types/Song";
import { calculateKeyShift } from "@/utils/shift-key";
import { Stepper } from "./stepper";

interface Props {
  song: Song;
  tempo: number;
  keyOffset: number;
  disabled?: boolean;
  onTempoChange: (tempo: number) => void;
  onKeyOffsetChange: (keyOffset: number) => void;
}

export const Shifts = ({
  song,
  tempo,
  keyOffset,
  disabled = false,
  onTempoChange,
  onKeyOffsetChange,
}: Props) => {
  if (!song.is_analyzed || song.transcript_source === "Usdx") return null;

  const shiftedKey = song.key ? calculateKeyShift(song.key, keyOffset).key : null;
  const controls = [
    {
      shiftType: "tempo" as const,
      title: "Tempo",
      description: "Adjust playback speed in 0.1× steps.",
      value: `${tempo.toFixed(1)}×`,
      onPlus: () => onTempoChange(Number((tempo + 0.1).toFixed(1))),
      onMinus: () => onTempoChange(Number((tempo - 0.1).toFixed(1))),
      disabled: { plus: tempo >= 2, minus: tempo <= 0.5 },
    },
    {
      shiftType: "key" as const,
      title: "Key",
      description: song.key ? `Original key: ${song.key}` : "Analyze again to detect the key.",
      value: shiftedKey,
      onPlus: () => onKeyOffsetChange(keyOffset + 1),
      onMinus: () => onKeyOffsetChange(keyOffset - 1),
      disabled: {
        plus: keyOffset >= 5 || !song.key,
        minus: keyOffset <= -5 || !song.key,
      },
    },
  ];

  return (
    <div className="divide-y">
      {controls.map(
        ({ shiftType, title, description, value, onPlus, onMinus, disabled: limit }) => (
          <div key={shiftType} className="flex flex-wrap items-center gap-x-3 gap-y-2 py-2">
            <div className="min-w-0 basis-48 flex-1">
              <p className="text-xs font-medium">{title}</p>
              <p className="mt-0.5 text-[0.625rem] leading-snug text-muted-foreground">
                {description}
              </p>
            </div>
            <Stepper
              ariaLabel={title.toLowerCase()}
              label={value}
              onClick={{ plus: onPlus, minus: onMinus }}
              disabled={{ plus: disabled || limit.plus, minus: disabled || limit.minus }}
            />
          </div>
        ),
      )}
    </div>
  );
};
