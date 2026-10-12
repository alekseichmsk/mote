import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { shouldShowGoLink } from "../lib/curious";
import { digitWordsOffer } from "../lib/numberWords";
import {
  preloadSpellcheck,
  spellOffer,
  spellcheckState,
  type SpellOffer,
} from "../lib/spellcheck";

export interface ExternalDigitOffer {
  digits: string;
  words: string;
}

export interface ExternalSpellOffer {
  /** Misspelled words, e.g. "превет, дила". */
  word: string;
  /** Corrected sentence or joined fixes. */
  suggestion: string;
  /** Full field text with every typo corrected. */
  replaced: string;
  /** Raw field text before dictionary — for LLM improve. */
  original: string;
}

interface SpeechBubbleProps {
  text: string;
  editing: boolean;
  input: string;
  busy: boolean;
  visible: boolean;
  /** Offer while typing digits in any other app. */
  externalDigitOffer?: ExternalDigitOffer | null;
  /** Offer while misspelling in any other app. */
  externalSpellOffer?: ExternalSpellOffer | null;
  onOpenEdit: () => void;
  onInputChange: (value: string) => void;
  onSubmit: () => void;
  onCancel: () => void;
  onCopyExternalDigit?: () => void;
  onReplaceExternalDigit?: () => void;
  onFactExternalDigit?: () => void;
  onDismissExternalDigit?: () => void;
  onCopyExternalSpell?: () => void;
  onReplaceExternalSpell?: () => void;
  onImproveExternalSpell?: () => void;
  onTypografExternalSpell?: () => void;
  onLayoutExternalSpell?: () => void;
  /** Remember word(s) — stop offering spell/layout on them. */
  onKeepExternalSpell?: () => void;
  onDismissExternalSpell?: () => void;
  /** LLM / download status line under spell actions. */
  spellStatus?: string | null;
  /** Open source page when a curious answer has a URL. */
  linkUrl?: string | null;
  onOpenLink?: () => void;
}

function IconReplace() {
  return (
    <svg viewBox="0 0 24 24" className="digit-action-icon" aria-hidden>
      <path
        d="M7 7h9.5a3.5 3.5 0 0 1 0 7H14"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M9.5 4.5L7 7l2.5 2.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M17 17H7.5a3.5 3.5 0 0 1 0-7H10"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M14.5 19.5L17 17l-2.5-2.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function IconCopy() {
  return (
    <svg viewBox="0 0 24 24" className="digit-action-icon" aria-hidden>
      <rect
        x="8.5"
        y="8.5"
        width="10"
        height="12"
        rx="2"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
      />
      <path
        d="M6.5 15.5H6A2 2 0 0 1 4 13.5v-9A2 2 0 0 1 6 2.5h8A2 2 0 0 1 16 4.5V5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  );
}

function IconDismiss() {
  return (
    <svg viewBox="0 0 20 20" className="digit-action-icon digit-action-icon-sm" aria-hidden>
      <path
        d="M5.2 5.2l9.6 9.6M14.8 5.2l-9.6 9.6"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
    </svg>
  );
}

function IconKeep() {
  return (
    <svg viewBox="0 0 24 24" className="digit-action-icon" aria-hidden>
      <path
        d="M6 4.5h12v16l-6-3.2L6 20.5v-16z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function IconImprove() {
  return (
    <svg viewBox="0 0 24 24" className="digit-action-icon" aria-hidden>
      <path
        d="M12 3l1.2 3.8L17 8l-3.8 1.2L12 13l-1.2-3.8L7 8l3.8-1.2L12 3z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinejoin="round"
      />
      <path
        d="M18 14l.7 2.1L21 17l-2.3.7L18 20l-.7-2.3L15 17l2.3-.9L18 14z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function IconTypograf() {
  return (
    <svg viewBox="0 0 24 24" className="digit-action-icon" aria-hidden>
      <path
        d="M5 7h14M12 7v12M8.5 19h7"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function IconLayout() {
  return (
    <svg viewBox="0 0 24 24" className="digit-action-icon" aria-hidden>
      <rect
        x="3.5"
        y="6"
        width="17"
        height="12"
        rx="2"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
      />
      <path
        d="M7 10h2M11 10h2M15 10h2M8 14h8"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
    </svg>
  );
}

function IconFact() {
  return (
    <svg viewBox="0 0 24 24" className="digit-action-icon" aria-hidden>
      <circle cx="12" cy="12" r="8.5" fill="none" stroke="currentColor" strokeWidth="2" />
      <path
        d="M12 10.5v5.5M12 8.2h.01"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  );
}

function IconGo() {
  return (
    <svg viewBox="0 0 20 20" className="speech-go-icon" aria-hidden>
      <path
        d="M7.2 4.5L12.7 10l-5.5 5.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function SpeechBubble({
  text,
  editing,
  input,
  busy,
  visible,
  externalDigitOffer = null,
  externalSpellOffer = null,
  onOpenEdit,
  onInputChange,
  onSubmit,
  onCancel,
  onCopyExternalDigit,
  onReplaceExternalDigit,
  onFactExternalDigit,
  onDismissExternalDigit,
  onCopyExternalSpell,
  onReplaceExternalSpell,
  onImproveExternalSpell,
  onTypografExternalSpell,
  onLayoutExternalSpell,
  onKeepExternalSpell,
  onDismissExternalSpell,
  spellStatus = null,
  linkUrl = null,
  onOpenLink,
}: SpeechBubbleProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const offer = useMemo(() => (editing ? digitWordsOffer(input) : null), [editing, input]);
  const [spell, setSpell] = useState<SpellOffer | null>(null);
  const [spellLoading, setSpellLoading] = useState(false);

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  useEffect(() => {
    preloadSpellcheck();
  }, []);

  useEffect(() => {
    if (!editing || offer) {
      setSpell(null);
      setSpellLoading(false);
      return;
    }
    const cyrillicWord = /[а-яёА-ЯЁ]{3,}/.test(input);
    let alive = true;
    let timer = 0;
    const run = () => {
      if (cyrillicWord && spellcheckState() !== "ready") setSpellLoading(true);
      void spellOffer(input).then((next) => {
        if (!alive) return;
        setSpell(next);
        setSpellLoading(spellcheckState() === "loading");
        if (next == null && spellcheckState() === "loading") {
          timer = window.setTimeout(run, 350);
        }
      });
    };
    timer = window.setTimeout(run, 120);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [editing, input, offer]);

  if (!visible) return null;

  function handleSubmit(e: FormEvent) {
    e.preventDefault();
    onSubmit();
  }

  function applyOffer() {
    if (!offer) return;
    onInputChange(offer.replaced);
    requestAnimationFrame(() => inputRef.current?.focus());
  }

  function applySpell(suggestion?: string) {
    if (!spell) return;
    const pick = suggestion ?? spell.suggestions[0];
    // Prefer full-sentence correction when applying the primary suggestion.
    const next =
      (!suggestion || suggestion === spell.suggestions[0]) && spell.replaced
        ? spell.replaced
        : input.slice(0, spell.start) + pick + input.slice(spell.end);
    onInputChange(next);
    setSpell(null);
    requestAnimationFrame(() => inputRef.current?.focus());
  }

  async function copyOffer() {
    if (!offer) return;
    try {
      await navigator.clipboard.writeText(offer.preview);
    } catch {
      /* ignore */
    }
  }

  if (editing) {
    return (
      <form className="speech speech-edit" data-hit onSubmit={handleSubmit}>
        <input
          ref={inputRef}
          value={input}
          disabled={busy}
          placeholder="Напиши…"
          aria-label="Сообщение Mote"
          onChange={(e) => onInputChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              onCancel();
              return;
            }
            if (e.key === "Tab" && offer) {
              e.preventDefault();
              applyOffer();
              return;
            }
            if (e.key === "Tab" && spell) {
              e.preventDefault();
              applySpell();
            }
          }}
        />
        {offer && (
          <div className="digit-inline">
            <span className="digit-inline-preview" title={offer.preview}>
              {offer.preview}
            </span>
            <div className="digit-offer-actions">
              <button
                type="button"
                className="digit-action-btn digit-action-btn-main"
                disabled={busy}
                onClick={applyOffer}
                aria-label="Заменить цифры прописью"
                title="Заменить"
              >
                <IconReplace />
              </button>
              <button
                type="button"
                className="digit-action-btn"
                disabled={busy}
                onClick={() => void copyOffer()}
                aria-label="Скопировать прописью"
                title="Скопировать"
              >
                <IconCopy />
              </button>
            </div>
          </div>
        )}
        {!offer && spell && (
          <div className="digit-inline spell-inline" data-hit>
            <span className="digit-inline-preview" title={spell.suggestions.join(", ")}>
              <span className="spell-bad">{spell.word}</span>
              <span className="digit-offer-arrow" aria-hidden>
                →
              </span>
              <span className="spell-suggestions">{spell.suggestions[0]}</span>
            </span>
            <div className="digit-offer-actions">
              <button
                type="button"
                className="digit-action-btn digit-action-btn-main"
                disabled={busy}
                onClick={() => applySpell()}
                aria-label="Исправить орфографию"
                title="Исправить (Tab)"
              >
                <IconReplace />
              </button>
              <button
                type="button"
                className="digit-action-btn digit-action-btn-ghost"
                disabled={busy}
                onClick={() => setSpell(null)}
                aria-label="Скрыть подсказку"
                title="Скрыть"
              >
                <IconDismiss />
              </button>
            </div>
          </div>
        )}
        {!offer && !spell && spellLoading && (
          <div className="digit-inline spell-inline spell-loading" data-hit>
            <span className="digit-inline-preview">Проверяю правописание…</span>
          </div>
        )}
      </form>
    );
  }

  if (externalDigitOffer) {
    return (
      <div
        className="speech speech-digit-offer"
        data-hit
        role="dialog"
        aria-label="Число прописью"
      >
        <p className="digit-offer-text">
          <span className="digit-offer-raw">{externalDigitOffer.digits}</span>
          <span className="digit-offer-arrow" aria-hidden>
            →
          </span>
          <span className="digit-offer-words">{externalDigitOffer.words}</span>
        </p>
        <div className="digit-offer-actions">
          <button
            type="button"
            className="digit-action-btn digit-action-btn-main"
            onPointerDown={(e) => {
              // WebView2: preventDefault on mousedown can swallow click — act here.
              e.preventDefault();
              onReplaceExternalDigit?.();
            }}
            aria-label="Заменить в поле ввода"
            title="Заменить"
          >
            <IconReplace />
          </button>
          <button
            type="button"
            className="digit-action-btn"
            onPointerDown={(e) => {
              e.preventDefault();
              onCopyExternalDigit?.();
            }}
            aria-label="Скопировать прописью"
            title="Скопировать"
          >
            <IconCopy />
          </button>
          {onFactExternalDigit && (
            <button
              type="button"
              className="digit-action-btn"
              onPointerDown={(e) => {
                e.preventDefault();
                onFactExternalDigit();
              }}
              aria-label="Факт о числе"
              title="Факт"
            >
              <IconFact />
            </button>
          )}
          <button
            type="button"
            className="digit-action-btn digit-action-btn-ghost"
            onPointerDown={(e) => {
              e.preventDefault();
              onDismissExternalDigit?.();
            }}
            aria-label="Закрыть"
            title="Закрыть"
          >
            <IconDismiss />
          </button>
        </div>
      </div>
    );
  }

  if (externalSpellOffer) {
    return (
      <div
        className="speech speech-digit-offer"
        data-hit
        role="dialog"
        aria-label="Исправить слово"
      >
        <p className="digit-offer-text">
          <span className="spell-bad">{externalSpellOffer.word}</span>
          <span className="digit-offer-arrow" aria-hidden>
            →
          </span>
          <span className="digit-offer-words">{externalSpellOffer.suggestion}</span>
        </p>
        <div className="digit-offer-actions" role="toolbar" aria-label="Действия с текстом">
          <div className="digit-offer-group">
            <button
              type="button"
              className="digit-action-btn digit-action-btn-main"
              onPointerDown={(e) => {
                e.preventDefault();
                onReplaceExternalSpell?.();
              }}
              aria-label="Заменить в поле ввода"
              title="Заменить"
            >
              <IconReplace />
            </button>
            <button
              type="button"
              className="digit-action-btn"
              onPointerDown={(e) => {
                e.preventDefault();
                onCopyExternalSpell?.();
              }}
              aria-label="Скопировать исправление"
              title="Скопировать"
            >
              <IconCopy />
            </button>
          </div>
          <div className="digit-offer-group">
            <button
              type="button"
              className="digit-action-btn"
              onPointerDown={(e) => {
                e.preventDefault();
                onImproveExternalSpell?.();
              }}
              aria-label="Улучшить текст (локальная модель)"
              title="Улучшить"
            >
              <IconImprove />
            </button>
            <button
              type="button"
              className="digit-action-btn"
              onPointerDown={(e) => {
                e.preventDefault();
                onTypografExternalSpell?.();
              }}
              aria-label="Типографика (кавычки, тире, пробелы)"
              title="Типограф"
            >
              <IconTypograf />
            </button>
            <button
              type="button"
              className="digit-action-btn"
              onPointerDown={(e) => {
                e.preventDefault();
                onLayoutExternalSpell?.();
              }}
              aria-label="Латиницу в выделении → русские буквы"
              title="Раскладка"
            >
              <IconLayout />
            </button>
            <button
              type="button"
              className="digit-action-btn"
              onPointerDown={(e) => {
                e.preventDefault();
                onKeepExternalSpell?.();
              }}
              aria-label="Оставить как есть и запомнить слово"
              title="Оставить"
            >
              <IconKeep />
            </button>
          </div>
          <div className="digit-offer-group digit-offer-group-end">
            <button
              type="button"
              className="digit-action-btn digit-action-btn-ghost"
              onPointerDown={(e) => {
                e.preventDefault();
                onDismissExternalSpell?.();
              }}
              aria-label="Закрыть"
              title="Закрыть"
            >
              <IconDismiss />
            </button>
          </div>
        </div>
        {spellStatus ? <p className="spell-status">{spellStatus}</p> : null}
      </div>
    );
  }

  const showGo = shouldShowGoLink(text, linkUrl) && !busy && !editing;

  return (
    <div
      className={`speech${busy ? " thinking" : ""}${showGo ? " speech-with-go" : ""}`}
      data-hit
      role="status"
      aria-live="polite"
    >
      <div
        className="speech-body"
        role="button"
        tabIndex={0}
        onClick={onOpenEdit}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onOpenEdit();
          }
        }}
      >
        {busy ? "…" : text}
      </div>
      {showGo && (
        <button
          type="button"
          className="speech-go"
          aria-label="Перейти"
          title="Перейти"
          onClick={(e) => {
            e.stopPropagation();
            onOpenLink?.();
          }}
        >
          <IconGo />
        </button>
      )}
    </div>
  );
}
