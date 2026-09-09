import { useState } from 'react';
import { toast } from 'sonner';

import { measureMicLatencySec } from '@/features/microphone/lib/latency-test';
import { Button } from '@/shared/components/ui/button';
import { Field } from '@/shared/components/ui/field';
import { Label } from '@/shared/components/ui/label';
import { Slider } from '@/shared/components/ui/slider';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/shared/components/ui/tooltip';

import { Hint } from './settings-controls';

type MicLatencyFieldProps = {
  selectedMicId: string | null;
  latencySec: number;
  sliderClassName?: string;
  buttonClassName?: string;
  disabled?: boolean;
  onMeasuringChange?: (measuring: boolean) => void;
  onLatencyChange: (latencySec: number) => void;
};

export function MicLatencyField({
  selectedMicId,
  latencySec,
  sliderClassName,
  buttonClassName,
  disabled = false,
  onMeasuringChange,
  onLatencyChange,
}: MicLatencyFieldProps) {
  const [measuring, setMeasuring] = useState(false);
  const latencyMs = Math.round(latencySec * 1000);

  const runTest = async () => {
    if (measuring) {
      return;
    }

    setMeasuring(true);
    onMeasuringChange?.(true);
    try {
      const measured = await measureMicLatencySec(selectedMicId);
      onLatencyChange(measured);
      toast.success(`Mic latency measured: ${Math.round(measured * 1000)} ms`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error(`Mic latency test failed: ${message}`);
    } finally {
      setMeasuring(false);
      onMeasuringChange?.(false);
    }
  };

  return (
    <Field>
      <Label>Mic latency compensation</Label>
      <Hint>Aligns mic pitch with guide vocals for scoring. Current value: {latencyMs} ms</Hint>
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <Slider
          min={0}
          max={500}
          step={5}
          value={[latencyMs]}
          onValueChange={([ms]) => onLatencyChange(ms / 1000)}
          className={sliderClassName}
        />
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="outline"
              onClick={() => void runTest()}
              disabled={disabled || measuring}
              className={buttonClassName}
            >
              {measuring ? 'Measuring...' : 'Measure latency'}
            </Button>
          </TooltipTrigger>
          <TooltipContent align="end">
            Plays a short beep, listens for it through the selected mic, then saves the delay used
            for scoring. Use speakers, or hold one headphone close to the mic.
          </TooltipContent>
        </Tooltip>
      </div>
    </Field>
  );
}
