import { FormEvent, useEffect, useRef, useState } from "react";
import { PetAvatar } from "./PetAvatar";

interface RageOverlayProps {
  petName: string;
  onForgive: () => void;
}

function isApology(raw: string): boolean {
  const t = raw
    .trim()
    .toLowerCase()
    .replace(/ё/g, "е")
    .replace(/[!?.,…\s]+$/g, "");
  return t === "прости";
}

export function RageOverlay({ petName, onForgive }: RageOverlayProps) {
  const [text, setText] = useState("");
  const [shake, setShake] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    const id = window.setTimeout(() => inputRef.current?.focus(), 280);
    return () => window.clearTimeout(id);
  }, []);

  function submit(e?: FormEvent) {
    e?.preventDefault();
    if (isApology(text)) {
      onForgive();
      return;
    }
    setShake(true);
    window.setTimeout(() => setShake(false), 420);
  }

  return (
    <div className="rage-screen" data-hit>
      <div className="rage-pet-wrap">
        <PetAvatar
          mood="angry"
          name={petName}
          musicPlaying={false}
          onDragStart={() => {}}
          onClick={() => {}}
        />
      </div>
      <p className="rage-title">ХВАТИТ МЕНЯ ГОНЯТЬ!</p>
      <p className="rage-hint">Напиши «прости» — и я успокоюсь.</p>
      <form
        className={`rage-form${shake ? " rage-form-shake" : ""}`}
        onSubmit={submit}
        data-hit
      >
        <input
          ref={inputRef}
          className="rage-input"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="прости"
          autoComplete="off"
          spellCheck={false}
          aria-label="Извинение"
        />
        <button type="submit" className="rage-submit">
          Сказать
        </button>
      </form>
    </div>
  );
}
