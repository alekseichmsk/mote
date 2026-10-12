import { CSSProperties, MouseEvent, useRef, useState } from "react";
import { PetMood } from "../lib/types";
import { PetRive } from "./PetRive";

interface PetAvatarProps {
  mood: PetMood;
  name: string;
  lookX?: number;
  lookY?: number;
  eyesTracking?: boolean;
  musicPlaying?: boolean;
  /** Freeze presence docking while the pointer is down / awaiting double-tap. */
  onPointerGesture?: (active: boolean) => void;
  onDragStart: () => void;
  onClick: () => void;
  onDoubleClick?: () => void;
  onContextMenu?: (e: MouseEvent) => void;
}

const NOTE_GLYPHS = ["♪", "♫", "♪", "♬", "♪", "♫"] as const;
const DRAG_THRESHOLD = 12;
const DBL_MS = 450;

export function PetAvatar({
  mood,
  name,
  lookX = 0,
  lookY = 0,
  eyesTracking = false,
  musicPlaying = false,
  onPointerGesture,
  onDragStart,
  onClick,
  onDoubleClick,
  onContextMenu,
}: PetAvatarProps) {
  const eyeStyle = {
    ["--look-x" as string]: `${lookX.toFixed(2)}px`,
    ["--look-y" as string]: `${lookY.toFixed(2)}px`,
  } as CSSProperties;

  // Screen space — webview client coords jump when the window docks to the caret.
  const dragOrigin = useRef<{ x: number; y: number } | null>(null);
  const dragging = useRef(false);
  const lastTap = useRef(0);
  const clickTimer = useRef<number | null>(null);
  const gesturePaused = useRef(false);
  // Rive community pack is a character sheet — keep SVG until custom mote.riv exists.
  const [riveFailed, setRiveFailed] = useState(true);

  function pausePresence() {
    if (gesturePaused.current) return;
    gesturePaused.current = true;
    onPointerGesture?.(true);
  }

  function resumePresence() {
    if (!gesturePaused.current) return;
    gesturePaused.current = false;
    onPointerGesture?.(false);
  }

  return (
    <button
      type="button"
      className={`pet-avatar mood-${mood}${eyesTracking ? " eyes-tracking" : ""}${
        musicPlaying ? " music-on" : ""
      }${riveFailed ? " pet-avatar-svg" : " pet-avatar-rive"}`}
      style={eyeStyle}
      data-hit
      aria-label={name}
      onPointerDown={(e) => {
        // Middle mouse (wheel click) → open compose immediately.
        if (e.button === 1) {
          e.preventDefault();
          if (clickTimer.current) {
            window.clearTimeout(clickTimer.current);
            clickTimer.current = null;
          }
          lastTap.current = 0;
          onClick();
          return;
        }
        if (e.button !== 0) return;
        const el = e.currentTarget;
        el.setPointerCapture?.(e.pointerId);
        pausePresence();
        dragOrigin.current = { x: e.screenX, y: e.screenY };
        dragging.current = false;
        const origin = dragOrigin.current;

        const cleanup = () => {
          el.removeEventListener("pointermove", onMove);
          el.removeEventListener("pointerup", onUp);
          el.removeEventListener("pointercancel", onUp);
        };

        const onMove = (ev: PointerEvent) => {
          if (!origin || dragging.current) return;
          const dx = ev.screenX - origin.x;
          const dy = ev.screenY - origin.y;
          if (dx * dx + dy * dy >= DRAG_THRESHOLD * DRAG_THRESHOLD) {
            dragging.current = true;
            lastTap.current = 0;
            if (clickTimer.current) {
              window.clearTimeout(clickTimer.current);
              clickTimer.current = null;
            }
            cleanup();
            // startDrag owns pause until drag ends.
            onDragStart();
          }
        };

        const onUp = () => {
          cleanup();
          dragOrigin.current = null;
          if (dragging.current) return;

          const now = Date.now();
          if (now - lastTap.current <= DBL_MS) {
            lastTap.current = 0;
            if (clickTimer.current) {
              window.clearTimeout(clickTimer.current);
              clickTimer.current = null;
            }
            resumePresence();
            onDoubleClick?.();
            return;
          }

          lastTap.current = now;
          // Keep presence paused until single-click fires or a second tap arrives.
          if (clickTimer.current) window.clearTimeout(clickTimer.current);
          clickTimer.current = window.setTimeout(() => {
            clickTimer.current = null;
            resumePresence();
            onClick();
          }, DBL_MS);
        };

        el.addEventListener("pointermove", onMove);
        el.addEventListener("pointerup", onUp);
        el.addEventListener("pointercancel", onUp);
      }}
      onContextMenu={onContextMenu}
    >
      {musicPlaying && (
        <div className="music-notes" aria-hidden>
          {NOTE_GLYPHS.map((glyph, i) => (
            <span key={i} className={`music-note n${i + 1}`}>
              {glyph}
            </span>
          ))}
        </div>
      )}
      {!riveFailed ? (
        <div className="pet-rive-wrap">
          <PetRive
            mood={mood}
            lookX={lookX}
            lookY={lookY}
            eyesTracking={eyesTracking}
            onFailed={() => setRiveFailed(true)}
          />
        </div>
      ) : null}
      {riveFailed ? (
      <svg
        viewBox="0 0 200 200"
        className="pet-svg"
        role="img"
        aria-hidden
        preserveAspectRatio="xMidYMid meet"
      >
        <defs>
          <clipPath id="cloud-clip">
            <ellipse cx="70" cy="108" rx="48" ry="44" />
            <ellipse cx="130" cy="108" rx="48" ry="44" />
            <ellipse cx="100" cy="88" rx="56" ry="52" />
            <ellipse cx="58" cy="78" rx="34" ry="32" />
            <ellipse cx="142" cy="78" rx="34" ry="32" />
            <ellipse cx="100" cy="128" rx="46" ry="36" />
          </clipPath>
        </defs>

        <g className="pet-body">
          <ellipse cx="70" cy="108" rx="48" ry="44" fill="#e21f7a" />
          <ellipse cx="130" cy="108" rx="48" ry="44" fill="#e21f7a" />
          <ellipse cx="100" cy="88" rx="56" ry="52" fill="#e21f7a" />
          <ellipse cx="58" cy="78" rx="34" ry="32" fill="#e21f7a" />
          <ellipse cx="142" cy="78" rx="34" ry="32" fill="#e21f7a" />
          <ellipse cx="100" cy="128" rx="46" ry="36" fill="#e21f7a" />
        </g>

        <g className="pet-face" clipPath="url(#cloud-clip)">
          <path
            className="pet-brow pet-brow-l"
            d="M66 78 L92 88"
            stroke="#0a0a0a"
            strokeWidth="7"
            strokeLinecap="round"
            fill="none"
          />
          <path
            className="pet-brow pet-brow-r"
            d="M134 78 L108 88"
            stroke="#0a0a0a"
            strokeWidth="7"
            strokeLinecap="round"
            fill="none"
          />

          <g className="pet-eyes">
            <rect className="pet-eye pet-eye-l" x="74" y="82" width="18" height="36" rx="9" fill="#0a0a0a" />
            <rect className="pet-eye pet-eye-r" x="108" y="82" width="18" height="36" rx="9" fill="#0a0a0a" />

            <g className="pet-glasses" aria-hidden>
              <circle
                cx="83"
                cy="100"
                r="18"
                fill="rgba(255,255,255,0.14)"
                stroke="#1a1520"
                strokeWidth="5"
              />
              <circle
                cx="117"
                cy="100"
                r="18"
                fill="rgba(255,255,255,0.14)"
                stroke="#1a1520"
                strokeWidth="5"
              />
              <path d="M101 100 H99" stroke="#1a1520" strokeWidth="3.5" strokeLinecap="round" />
              <path
                d="M65 100 Q60 99 56 97"
                stroke="#1a1520"
                strokeWidth="3.5"
                strokeLinecap="round"
                fill="none"
              />
              <path
                d="M135 100 Q140 99 144 97"
                stroke="#1a1520"
                strokeWidth="3.5"
                strokeLinecap="round"
                fill="none"
              />
            </g>

            <path
              className="pet-laugh-eye pet-laugh-l"
              d="M72 102 Q83 88 94 102"
              stroke="#0a0a0a"
              strokeWidth="8"
              strokeLinecap="round"
              fill="none"
            />
            <path
              className="pet-laugh-eye pet-laugh-r"
              d="M106 102 Q117 88 128 102"
              stroke="#0a0a0a"
              strokeWidth="8"
              strokeLinecap="round"
              fill="none"
            />

            <ellipse className="pet-tear pet-tear-l" cx="80" cy="126" rx="4.5" ry="7" fill="#4eb8ff" />
            <ellipse className="pet-tear pet-tear-l2" cx="88" cy="132" rx="3.2" ry="5" fill="#7ad0ff" />
            <ellipse className="pet-tear pet-tear-r" cx="120" cy="128" rx="4.5" ry="7" fill="#4eb8ff" />
            <ellipse className="pet-tear pet-tear-r2" cx="112" cy="134" rx="3.2" ry="5" fill="#7ad0ff" />
          </g>
        </g>
      </svg>
      ) : null}
    </button>
  );
}
