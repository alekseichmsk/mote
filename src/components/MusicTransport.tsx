import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { invoke } from "@tauri-apps/api/core";

interface MusicTransportProps {
  playing: boolean;
  onPrevious: () => void;
  onToggle: () => void;
  onNext: () => void;
  onVolumePanelChange?: (open: boolean) => void;
}

function isTauri() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function IconPrev() {
  return (
    <svg viewBox="0 0 24 24" className="mote-media-icon" aria-hidden>
      <path
        d="M11.2 6.2L5.4 12l5.8 5.8"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M17.6 6.2L11.8 12l5.8 5.8"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity="0.55"
      />
    </svg>
  );
}

function IconNext() {
  return (
    <svg viewBox="0 0 24 24" className="mote-media-icon" aria-hidden>
      <path
        d="M12.8 6.2L18.6 12l-5.8 5.8"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M6.4 6.2L12.2 12l-5.8 5.8"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity="0.55"
      />
    </svg>
  );
}

function IconPlay() {
  return (
    <svg viewBox="0 0 24 24" className="mote-media-icon mote-media-icon-main" aria-hidden>
      <path d="M9 6.2v11.6L18.4 12 9 6.2z" fill="currentColor" />
    </svg>
  );
}

function IconPause() {
  return (
    <svg viewBox="0 0 24 24" className="mote-media-icon mote-media-icon-main" aria-hidden>
      <rect x="7.2" y="6" width="3.2" height="12" rx="1.2" fill="currentColor" />
      <rect x="13.6" y="6" width="3.2" height="12" rx="1.2" fill="currentColor" />
    </svg>
  );
}

function IconVolume() {
  return (
    <svg viewBox="0 0 24 24" className="mote-media-icon" aria-hidden>
      <path d="M4.5 9.5h3.2L12 5.8v12.4l-4.3-3.7H4.5V9.5z" fill="currentColor" />
      <path
        d="M15.2 9.2c1.1.9 1.8 2.2 1.8 3.8s-.7 2.9-1.8 3.8"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function MusicTransport({
  playing,
  onPrevious,
  onToggle,
  onNext,
  onVolumePanelChange,
}: MusicTransportProps) {
  const [volOpen, setVolOpen] = useState(false);
  const [flipLeft, setFlipLeft] = useState(false);
  const [level, setLevel] = useState(0.7);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const trackRef = useRef<HTMLDivElement | null>(null);
  const dragging = useRef(false);
  const lastSent = useRef(0);

  const applyLevel = useCallback(async (next: number) => {
    const clamped = Math.max(0, Math.min(1, next));
    setLevel(clamped);
    if (!isTauri()) return;
    const now = performance.now();
    if (now - lastSent.current < 24 && clamped > 0.001 && clamped < 0.999) return;
    lastSent.current = now;
    try {
      await invoke("media_volume_set", { level: clamped });
    } catch {
      /* ignore — overlay stays usable */
    }
  }, []);

  const levelFromPointer = useCallback((clientY: number) => {
    const el = trackRef.current;
    if (!el) return null;
    const rect = el.getBoundingClientRect();
    if (rect.height <= 0) return null;
    return 1 - (clientY - rect.top) / rect.height;
  }, []);

  useEffect(() => {
    if (!volOpen || !isTauri()) return;
    let cancelled = false;
    void invoke<number>("media_volume_get")
      .then((v) => {
        if (!cancelled && Number.isFinite(v)) setLevel(Math.max(0, Math.min(1, v)));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [volOpen]);

  useEffect(() => {
    if (!volOpen) {
      setFlipLeft(false);
      return;
    }
    const id = window.requestAnimationFrame(() => {
      const root = rootRef.current;
      if (!root) return;
      const r = root.getBoundingClientRect();
      setFlipLeft(r.right + 40 > window.innerWidth - 8);
    });
    return () => window.cancelAnimationFrame(id);
  }, [volOpen]);

  function toggleVol() {
    setVolOpen((v) => {
      const next = !v;
      onVolumePanelChange?.(next);
      return next;
    });
  }

  function onTrackPointerDown(e: ReactPointerEvent<HTMLDivElement>) {
    e.preventDefault();
    e.stopPropagation();
    dragging.current = true;
    const next = levelFromPointer(e.clientY);
    if (next != null) {
      lastSent.current = 0;
      void applyLevel(next);
    }

    const onMove = (ev: PointerEvent) => {
      if (!dragging.current) return;
      const v = levelFromPointer(ev.clientY);
      if (v != null) void applyLevel(v);
    };
    const onUp = (ev: PointerEvent) => {
      dragging.current = false;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      const v = levelFromPointer(ev.clientY);
      if (v != null) {
        lastSent.current = 0;
        void applyLevel(v);
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  }

  const pct = Math.round(level * 100);

  return (
    <div
      ref={rootRef}
      className={`mote-media${volOpen ? " mote-media-vol-open" : ""}`}
      role="group"
      aria-label="Управление музыкой"
    >
      <div className="mote-media-row">
        <button
          type="button"
          className="mote-media-btn"
          aria-label="Предыдущий трек"
          onClick={onPrevious}
        >
          <IconPrev />
        </button>
        <button
          type="button"
          className="mote-media-btn mote-media-btn-main"
          aria-label={playing ? "Пауза" : "Играть"}
          onClick={onToggle}
        >
          {playing ? <IconPause /> : <IconPlay />}
        </button>
        <button
          type="button"
          className="mote-media-btn"
          aria-label="Следующий трек"
          onClick={onNext}
        >
          <IconNext />
        </button>
        <button
          type="button"
          className={`mote-media-btn mote-vol-speaker${volOpen ? " mote-media-btn-active" : ""}`}
          aria-label={volOpen ? "Скрыть громкость" : "Громкость"}
          aria-expanded={volOpen}
          onClick={toggleVol}
        >
          <IconVolume />
        </button>
      </div>
      {volOpen && (
        <div
          className={`mote-media-vol${flipLeft ? " mote-media-vol-flip" : ""}`}
          role="group"
          aria-label="Громкость Windows"
          data-hit
        >
          <div
            ref={trackRef}
            className="mote-vol-scale"
            role="slider"
            tabIndex={0}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={pct}
            aria-orientation="vertical"
            aria-label="Громкость"
            style={{ ["--vol" as string]: String(pct) }}
            onPointerDown={onTrackPointerDown}
            onKeyDown={(e) => {
              if (e.key === "ArrowUp" || e.key === "ArrowRight") {
                e.preventDefault();
                lastSent.current = 0;
                void applyLevel(level + 0.05);
              } else if (e.key === "ArrowDown" || e.key === "ArrowLeft") {
                e.preventDefault();
                lastSent.current = 0;
                void applyLevel(level - 0.05);
              }
            }}
          >
            <div className="mote-vol-rail" />
            <div className="mote-vol-fill" />
            <div className="mote-vol-thumb" />
          </div>
        </div>
      )}
    </div>
  );
}
