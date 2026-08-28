import { usePixabaySlots } from "@/hooks/use-pixabay-slots";
import type { VideoFlavor } from "@/lib/playback/video-flavor";
import { VIDEO_CLASS_NAME } from "@/lib/playback/video-styles";

interface PixabayVideoProps {
  flavor: VideoFlavor;
  isPlaying: boolean;
  /** When false (the default), the active clip loops instead of cutting to
   * a different one from the pool each time it ends -- a native `loop`
   * attribute short-circuits the `ended` event entirely, so
   * `usePixabaySlots`'s clip-switching logic in `onEnded` simply never
   * fires; no change needed there. */
  rotationEnabled?: boolean;
}

export const PixabayVideo = ({ flavor, isPlaying, rotationEnabled = false }: PixabayVideoProps) => {
  const { slots, onActiveEnded } = usePixabaySlots(flavor, isPlaying);

  return (
    <>
      {slots.map((slot, i) => (
        <video
          key={i}
          ref={slot.ref}
          className={VIDEO_CLASS_NAME}
          style={{ visibility: slot.isActive ? "visible" : "hidden" }}
          src={slot.src || undefined}
          preload="auto"
          muted
          playsInline
          loop={!rotationEnabled}
          onEnded={rotationEnabled && slot.isActive ? onActiveEnded : undefined}
          onError={slot.isActive ? onActiveEnded : undefined}
        />
      ))}
    </>
  );
};
