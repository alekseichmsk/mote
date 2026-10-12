import { MusicTransport } from "./MusicTransport";

interface ContextMenuProps {
  musicPlaying: boolean;
  onToggleMusic: () => void;
  onPreviousTrack: () => void;
  onNextTrack: () => void;
  onVolumePanelChange?: (open: boolean) => void;
}

/** Compact media bar centered under the pet (ПКМ). Compose stays on middle-click. */
export function ContextMenu({
  musicPlaying,
  onToggleMusic,
  onPreviousTrack,
  onNextTrack,
  onVolumePanelChange,
}: ContextMenuProps) {
  return (
    <div
      className="mote-menu mote-menu-media-only"
      role="menu"
      aria-label="Управление музыкой"
      data-hit
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      <MusicTransport
        playing={musicPlaying}
        onPrevious={onPreviousTrack}
        onToggle={onToggleMusic}
        onNext={onNextTrack}
        onVolumePanelChange={onVolumePanelChange}
      />
    </div>
  );
}
