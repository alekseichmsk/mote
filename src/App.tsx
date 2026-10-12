import { useEffect, useMemo, useRef, useState, MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { ContextMenu } from "./components/ContextMenu";
import { PetAvatar } from "./components/PetAvatar";
import { RageOverlay } from "./components/RageOverlay";
import {
  ExternalDigitOffer,
  ExternalSpellOffer,
  SpeechBubble,
} from "./components/SpeechBubble";
import { ChatEngine, createMessage } from "./lib/llm/chatEngine";
import { numberToRussianWords } from "./lib/numberWords";
import { preloadSpellcheck } from "./lib/spellcheck";
import { applyTypograf, MAX_TYPOGRAF_CHARS } from "./lib/typograf";
import {
  fetchNumberFact,
  fetchQuote,
  fetchWikiRandom,
  openCuriousUrl,
  tryCuriousAsk,
} from "./lib/curious";
import { cityFromWeatherAsk, fetchWeather, isWeatherAsk } from "./lib/weather";
import { loadChat, loadSettings, saveChat, saveSettings } from "./lib/storage";
import {
  ChatMessage,
  detectMood,
  isIdleMood,
  MoteSettings,
  PetMood,
} from "./lib/types";

const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const GREETING = "Привет. Спроси: вики путин — или просто напиши мне.";
const WATCHING_PHRASE = "Смотрю рядом.";
const MUSIC_FALLBACK = "Слушаю с тобой.";

/** Max eye travel inside the cloud (SVG px). */
const LOOK_RANGE_X = 40;
const LOOK_RANGE_Y = 34;
/** Stronger stare while locked on a text caret. */
const LOOK_RANGE_TYPE_X = 48;
const LOOK_RANGE_TYPE_Y = 40;

function formatMusicPhrase(artist: string, title: string): string {
  const a = artist.trim();
  const t = title.trim();
  if (a && t) return `Слушаю: ${a} — ${t}`;
  if (t) return `Слушаю: ${t}`;
  if (a) return `Слушаю: ${a}`;
  return MUSIC_FALLBACK;
}

function isMusicPhrase(text: string): boolean {
  return text === MUSIC_FALLBACK || text.startsWith("Слушаю:");
}

/** Progress line + optional current track (second line) while the GGUF downloads. */
function downloadBubbleText(progressLine: string, musicPhrase: string): string {
  const progress = progressLine.trim() || "Скачиваю модель…";
  const music = musicPhrase.trim();
  return music ? `${progress}\n${music}` : progress;
}

const IDLE_ACTIONS: PetMood[] = [
  "glanceLeft",
  "glanceRight",
  "glanceUp",
  "curious",
  "drowsy",
  "peek",
  "glanceLeft",
  "glanceRight",
];

function pickIdle(): PetMood {
  return IDLE_ACTIONS[Math.floor(Math.random() * IDLE_ACTIONS.length)];
}

function rand(min: number, max: number) {
  return min + Math.random() * (max - min);
}

export default function App() {
  const engine = useMemo(() => new ChatEngine(), []);
  const [settings, setSettings] = useState<MoteSettings>(() => {
    const s = loadSettings();
    saveSettings(s);
    return s;
  });
  const [messages, setMessages] = useState<ChatMessage[]>(() => loadChat());
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [mood, setMood] = useState<PetMood>("idle");
  const [bubbleText, setBubbleText] = useState(GREETING);
  const [bubbleLink, setBubbleLink] = useState<string | null>(null);
  const [bubbleVisible, setBubbleVisible] = useState(true);
  const [editing, setEditing] = useState(false);
  const [look, setLook] = useState({ x: 0, y: 0 });
  const [eyesTracking, setEyesTracking] = useState(false);
  const [musicPlaying, setMusicPlaying] = useState(false);
  const [externalDigitOffer, setExternalDigitOffer] = useState<ExternalDigitOffer | null>(null);
  const [externalSpellOffer, setExternalSpellOffer] = useState<ExternalSpellOffer | null>(null);
  const [menu, setMenu] = useState(false);
  const [spellStatus, setSpellStatus] = useState<string | null>(null);
  const [volumePanelOpen, setVolumePanelOpen] = useState(false);
  const [raging, setRaging] = useState(false);
  const hideTimer = useRef<number | null>(null);
  const dragMoved = useRef(false);
  const busyRef = useRef(false);
  const editingRef = useRef(false);
  const ragingRef = useRef(false);
  const followUntil = useRef(0);
  const emotionUntil = useRef(0);
  const lookTarget = useRef({ x: 0, y: 0 });
  const lookCurrent = useRef({ x: 0, y: 0 });
  const digitOfferRef = useRef(false);
  const spellOfferRef = useRef(false);
  const musicPlayingRef = useRef(false);
  const watchingRef = useRef(false);
  /** Suppress music plaque while user is typing / spell offer is up. */
  const typingUntil = useRef(0);
  /** Glasses stay only while type heartbeats arrive; cleared on untarget / timeout. */
  const glassesUntil = useRef(0);
  /** Last music plaque text we announced — avoid re-showing on SMTC heartbeat. */
  const musicAnnounceRef = useRef("");
  /** After digit/spell dismiss — don't flash «Слушаю: …» over the empty bubble. */
  const musicPlaqueQuietUntil = useRef(0);
  /** While GGUF downloads — bubble shows progress (+ track on the second line). */
  const modelDownloadActiveRef = useRef(false);
  /** Latest «Скачиваю модель… N%» without the music line. */
  const modelDownloadLineRef = useRef("");
  /** Recent double-click shoo timestamps (ms). */
  const shooTimes = useRef<number[]>([]);
  /** Last curious API link (open via «открой»). */
  const lastCuriousUrl = useRef<string | null>(null);
  const lastCuriousIdleAt = useRef(0);
  /** Session shoo tally — 5 → fullscreen tantrum. */
  const shooCount = useRef(0);

  ragingRef.current = raging;

  busyRef.current = busy;
  editingRef.current = editing;
  digitOfferRef.current = externalDigitOffer != null;
  spellOfferRef.current = externalSpellOffer != null;
  musicPlayingRef.current = musicPlaying;
  watchingRef.current = bubbleText === WATCHING_PHRASE;

  function preferMusicEyes() {
    if (!musicPlayingRef.current) return;
    // Don't stay in glasses after leaving a text field — music wins over stale focus.
    if (
      editingRef.current ||
      busyRef.current ||
      digitOfferRef.current ||
      spellOfferRef.current ||
      watchingRef.current ||
      Date.now() < typingUntil.current ||
      Date.now() < emotionUntil.current
    ) {
      return;
    }
    setMood((m) =>
      isIdleMood(m) || m === "curious" || m === "music" || m === "focus" ? "music" : m,
    );
  }

  /** Hide digit/spell plaque without letting SMTC immediately show «Слушаю: …». */
  function quietMusicPlaque(ms = 2800) {
    musicPlaqueQuietUntil.current = Date.now() + ms;
  }

  function dismissDigitOfferUi() {
    quietMusicPlaque();
    setExternalDigitOffer(null);
    setBubbleVisible(false);
    setBubbleText("");
    setBubbleLink(null);
  }

  // Report interactive bounds so Rust can click-through empty window chrome.
  useEffect(() => {
    if (!isTauri()) return;
    let cancelled = false;

    const sync = () => {
      if (cancelled) return;
      const nodes = document.querySelectorAll("[data-hit]");
      const regions = Array.from(nodes)
        .map((el) => {
          const r = el.getBoundingClientRect();
          return { x: r.x, y: r.y, w: r.width, h: r.height };
        })
        .filter((r) => r.w > 1 && r.h > 1);
      const menuEl = document.querySelector(".mote-menu");
      let menuRect: { x: number; y: number; w: number; h: number } | null = null;
      if (menuEl) {
        const r = menuEl.getBoundingClientRect();
        if (r.width > 1 && r.height > 1) {
          menuRect = { x: r.x, y: r.y, w: r.width, h: r.height };
        }
      }
      // Volume flyout sits outside the menu box — include it in hit-test.
      const volEl = document.querySelector(".mote-media-vol");
      if (volEl) {
        const r = volEl.getBoundingClientRect();
        if (r.width > 1 && r.height > 1) {
          regions.push({ x: r.x, y: r.y, w: r.width, h: r.height });
          if (menuRect) {
            const left = Math.min(menuRect.x, r.x);
            const top = Math.min(menuRect.y, r.y);
            const right = Math.max(menuRect.x + menuRect.w, r.x + r.width);
            const bottom = Math.max(menuRect.y + menuRect.h, r.y + r.height);
            menuRect = { x: left, y: top, w: right - left, h: bottom - top };
          }
        }
      }
      void invoke("set_hit_regions", { regions, menu: menuRect });
    };

    sync();
    const id = window.setInterval(sync, 80);
    window.addEventListener("resize", sync);
    return () => {
      cancelled = true;
      window.clearInterval(id);
      window.removeEventListener("resize", sync);
    };
  }, [menu, bubbleVisible, editing, externalDigitOffer, externalSpellOffer, musicPlaying, volumePanelOpen, raging]);

  useEffect(() => {
    preloadSpellcheck();
  }, []);

  // Once a day after 10:00 — Russian offline quote (one plaque per calendar day).
  useEffect(() => {
    if (!isTauri()) return;
    const now = new Date();
    if (now.getHours() < 10) return;
    const key = `mote-quote-day-${now.toDateString()}`;
    if (localStorage.getItem(key)) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void fetchQuote()
        .then((line) => {
          if (cancelled || busyRef.current || editingRef.current) return;
          if (digitOfferRef.current || spellOfferRef.current) return;
          localStorage.setItem(key, "1");
          lastCuriousUrl.current = line.url ?? null;
          showBubble(line.text, line.url ?? null);
          setMood("curious");
          emotionUntil.current = Date.now() + 5000;
          scheduleHide(9000);
        })
        .catch(() => {
          /* skip */
        });
    }, 2500);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once on mount
  }, []);

  // Smooth eye aim ~144fps toward the latest look target.
  useEffect(() => {
    let raf = 0;
    let prev = performance.now();
    let wasTracking = false;

    const tick = (now: number) => {
      const dt = Math.min(0.05, (now - prev) / 1000);
      prev = now;

      const tracking = Date.now() < followUntil.current && Date.now() >= emotionUntil.current;
      const wantMusicEyes =
        musicPlayingRef.current &&
        !tracking &&
        !editingRef.current &&
        !busyRef.current &&
        !digitOfferRef.current &&
        !watchingRef.current &&
        Date.now() >= emotionUntil.current;

      const tx = tracking ? lookTarget.current.x : 0;
      const ty = tracking ? lookTarget.current.y : 0;
      // Higher lambda while tracking for snappy follow; softer return to center.
      const lambda = tracking ? 18 : 10;
      const a = 1 - Math.exp(-lambda * dt);
      const cx = lookCurrent.current.x + (tx - lookCurrent.current.x) * a;
      const cy = lookCurrent.current.y + (ty - lookCurrent.current.y) * a;
      lookCurrent.current = { x: cx, y: cy };

      const arrived = Math.abs(cx) < 0.15 && Math.abs(cy) < 0.15 && !tracking;
      // Music eyes use CSS animation — drop vector tracking overlay while listening.
      setEyesTracking((tracking || !arrived) && !wantMusicEyes);
      setLook((prevLook) =>
        Math.abs(prevLook.x - cx) > 0.08 || Math.abs(prevLook.y - cy) > 0.08
          ? { x: cx, y: cy }
          : prevLook,
      );
      // Drop glasses once when leaving an input lock or glasses TTL expired.
      if (
        (wasTracking && !tracking) ||
        (glassesUntil.current > 0 && Date.now() >= glassesUntil.current)
      ) {
        if (glassesUntil.current > 0 && Date.now() >= glassesUntil.current) {
          glassesUntil.current = 0;
          typingUntil.current = 0;
        }
        setMood((m) => (m === "focus" ? (musicPlayingRef.current ? "music" : "idle") : m));
      }
      wasTracking = tracking;

      raf = requestAnimationFrame(tick);
    };

    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  useEffect(() => {
    engine.setProvider(settings.provider, settings.deepseekApiKey);
  }, [engine, settings]);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void listen<MoteSettings>("mote://settings-changed", (event) => {
      setSettings((prev) => {
        const next = { ...prev, ...event.payload };
        saveSettings(next);
        return next;
      });
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    saveChat(messages);
  }, [messages]);

  useEffect(() => {
    if (!isTauri()) return;
    void invoke("set_presence_paused", { paused: editing || busy || raging });
  }, [editing, busy, raging]);

  useEffect(() => {
    if (!isTauri()) return;

    const unsubs: Array<() => void> = [];

    void listen("mote://focus-reply", () => {
      setBubbleVisible(true);
      setEditing(true);
    }).then((fn) => unsubs.push(fn));

    void listen("mote://menu-dismiss", () => {
      setVolumePanelOpen(false);
      setMenu(false);
    }).then((fn) => unsubs.push(fn));

    void listen<{ phase: string; percent: number; detail: string }>(
      "mote://model-progress",
      (event) => {
        const { phase, percent, detail } = event.payload;
        if (phase === "download") {
          const first = !modelDownloadActiveRef.current;
          modelDownloadActiveRef.current = true;
          const kind = detail.includes("движок")
            ? "Скачиваю движок"
            : detail.includes("модель")
              ? "Скачиваю модель"
              : "Скачиваю";
          const line = `${kind}… ${percent}%`;
          if (line === modelDownloadLineRef.current && !first) return;
          modelDownloadLineRef.current = line;
          setSpellStatus(line);
          // Don't restart mood/animation on every percent — only open the plaque once.
          if (first) {
            setBubbleLink(null);
            setBubbleVisible(true);
            setMood("curious");
          }
          const music = musicPlayingRef.current ? musicAnnounceRef.current : "";
          const next = downloadBubbleText(line, music);
          setBubbleText((t) => (t === next ? t : next));
          if (hideTimer.current) window.clearTimeout(hideTimer.current);
        } else if (phase === "ready") {
          modelDownloadActiveRef.current = false;
          modelDownloadLineRef.current = "";
          const line = detail || "Модель готова";
          setSpellStatus(line);
          setBubbleText(line);
          setBubbleLink(null);
          setBubbleVisible(true);
          window.setTimeout(() => {
            setSpellStatus(null);
            setBubbleVisible(false);
          }, 2500);
        } else if (phase === "error") {
          modelDownloadActiveRef.current = false;
          modelDownloadLineRef.current = "";
          const line = detail || "Ошибка загрузки модели";
          setSpellStatus(line);
          setBubbleText(line);
          setBubbleLink(null);
          setBubbleVisible(true);
        }
      },
    ).then((fn) => unsubs.push(fn));

    void listen<string>("mote://selection-spell-status", (event) => {
      const msg = (event.payload || "").trim() || "не удалось проверить";
      setBubbleText(msg);
      setBubbleLink(null);
      setBubbleVisible(true);
      setMood("curious");
      if (hideTimer.current) window.clearTimeout(hideTimer.current);
      hideTimer.current = window.setTimeout(() => setBubbleVisible(false), 4000);
    }).then((fn) => unsubs.push(fn));

    void listen<{ dir: string; mode: string; nx: number; ny: number }>("mote://look", (event) => {
      if (Date.now() < emotionUntil.current) return;
      const mode = event.payload.mode;
      const typing = mode === "type";
      const leftInput = mode === "untarget";

      if (leftInput) {
        glassesUntil.current = 0;
        typingUntil.current = 0;
        followUntil.current = 0;
        lookTarget.current = { x: 0, y: 0 };
        setMood((m) =>
          m === "focus" ? (musicPlayingRef.current ? "music" : "idle") : m,
        );
        // Music plaque / eyes can resume immediately after leaving an editor.
        preferMusicEyes();
        return;
      }

      if (typing) {
        typingUntil.current = Date.now() + 900;
        // Must keep getting type ticks (~140ms) or glasses drop within ~0.7s.
        glassesUntil.current = Date.now() + 700;
      }
      const hold =
        typing
          ? 900
          : mode === "dock" || mode === "interest"
            ? 1600
            : 1200;
      followUntil.current = Date.now() + hold;
      const nx = Number.isFinite(event.payload.nx) ? event.payload.nx : 0;
      const ny = Number.isFinite(event.payload.ny) ? event.payload.ny : 0;
      const rx = typing ? LOOK_RANGE_TYPE_X : LOOK_RANGE_X;
      const ry = typing ? LOOK_RANGE_TYPE_Y : LOOK_RANGE_Y;
      lookTarget.current = {
        x: Math.max(-1, Math.min(1, nx)) * rx,
        y: Math.max(-1, Math.min(1, ny)) * ry,
      };
      // At an input: glasses + stare. Elsewhere — no glasses.
      if (typing) {
        setMood((m) =>
          isIdleMood(m) || m === "curious" || m === "music" || m === "focus" ? "focus" : m,
        );
      } else {
        glassesUntil.current = 0;
        setMood((m) => {
          if (m !== "focus" && !isIdleMood(m) && m !== "curious" && m !== "music") return m;
          return musicPlayingRef.current ? "music" : "idle";
        });
      }
    }).then((fn) => unsubs.push(fn));

    void listen<{ reason: string; video: boolean }>("mote://activate", (event) => {
      if (!event.payload.video) {
        // Left YouTube/etc. — drop the watching phrase if it's still up.
        setBubbleText((t) => {
          if (t === WATCHING_PHRASE) {
            setBubbleVisible(false);
          }
          return t === WATCHING_PHRASE ? GREETING : t;
        });
        return;
      }
      if (Date.now() < emotionUntil.current) return;
      // Don't force vector-tracking here — let curious eye animation play.
      followUntil.current = 0;
      lookTarget.current = { x: 0, y: 0 };
      setMood("curious");
      showBubble(WATCHING_PHRASE, null);
      // Video wins the bubble even if SMTC already showed «Слушаю: …».
      if (hideTimer.current) window.clearTimeout(hideTimer.current);
    }).then((fn) => unsubs.push(fn));

    void listen<{ digits: string }>("mote://digit-offer", (event) => {
      if (editingRef.current || busyRef.current) return;
      const digits = (event.payload.digits || "").replace(/\D/g, "");
      if (!digits || digits.length > 12) {
        // Same as spell clear: don't leave a stale speech plaque or flash music.
        setExternalDigitOffer((prev) => {
          if (prev != null) {
            quietMusicPlaque();
            setBubbleVisible(false);
            setBubbleText("");
            setBubbleLink(null);
          }
          return null;
        });
        return;
      }
      try {
        const words = numberToRussianWords(BigInt(digits));
        setExternalSpellOffer(null);
        setBubbleText("");
        setBubbleLink(null);
        setExternalDigitOffer({ digits, words });
        setBubbleVisible(true);
        setMood("curious");
        if (hideTimer.current) window.clearTimeout(hideTimer.current);
      } catch {
        setExternalDigitOffer(null);
        setBubbleVisible(false);
      }
    }).then((fn) => unsubs.push(fn));

    void listen<{
      word: string;
      suggestions: string[];
      replaced: string;
      original?: string;
    }>("mote://spell-offer", (event) => {
      if (editingRef.current || busyRef.current) return;
      const word = (event.payload.word || "").trim();
      const suggestion =
        (event.payload.suggestions && event.payload.suggestions[0]) || "";
      const replaced = event.payload.replaced || suggestion;
      const original = (event.payload.original || "").trim() || replaced;
      if (!word || !suggestion) {
        spellOfferRef.current = false;
        setExternalSpellOffer((prev) => {
          if (prev != null) {
            // Don't leave greeting / music plaque up after a spell offer ends.
            quietMusicPlaque();
            setBubbleVisible(false);
          }
          return null;
        });
        return;
      }
      // Sync before React paint so music SMTC can't race past the offer.
      spellOfferRef.current = true;
      typingUntil.current = Date.now() + 14_000;
      setExternalDigitOffer(null);
      setExternalSpellOffer({ word, suggestion, replaced, original });
      setBubbleVisible(true);
      setMood("curious");
      if (hideTimer.current) window.clearTimeout(hideTimer.current);
    }).then((fn) => unsubs.push(fn));

    void listen<{ playing: boolean; title: string; artist: string }>("mote://music", (event) => {
      const playing = !!event.payload.playing;
      setMusicPlaying(playing);
      musicPlayingRef.current = playing;

      if (!playing) {
        // Keep announce during download — SMTC often flickers paused between ticks.
        if (!modelDownloadActiveRef.current) {
          musicAnnounceRef.current = "";
        }
        setBubbleText((t) => {
          if (modelDownloadActiveRef.current) {
            const line = modelDownloadLineRef.current;
            const next = line
              ? downloadBubbleText(line, musicAnnounceRef.current)
              : t;
            return next === t ? t : next;
          }
          if (isMusicPhrase(t)) {
            setBubbleVisible(false);
            return GREETING;
          }
          return t;
        });
        setMood((m) => (m === "music" ? "idle" : m));
        return;
      }

      // Notes always run via musicPlaying; eyes only if not distracted.
      preferMusicEyes();

      const phrase = formatMusicPhrase(event.payload.artist ?? "", event.payload.title ?? "");

      // During model download: keep progress + show/update track on the second line.
      if (modelDownloadActiveRef.current) {
        musicAnnounceRef.current = phrase;
        const next = downloadBubbleText(modelDownloadLineRef.current, phrase);
        setBubbleText((t) => (t === next ? t : next));
        setBubbleLink(null);
        setBubbleVisible(true);
        if (hideTimer.current) window.clearTimeout(hideTimer.current);
        return;
      }

      // Digit / spell / typing / emotion / post-dismiss quiet: remember track, don't flash.
      if (
        editingRef.current ||
        busyRef.current ||
        digitOfferRef.current ||
        spellOfferRef.current ||
        Date.now() < typingUntil.current ||
        Date.now() < emotionUntil.current ||
        Date.now() < musicPlaqueQuietUntil.current
      ) {
        musicAnnounceRef.current = phrase;
        return;
      }

      // Only flash the plaque on track change — not on every SMTC heartbeat.
      if (phrase === musicAnnounceRef.current) return;

      let applied = false;
      let blockedByVideo = false;
      setBubbleText((t) => {
        if (t === WATCHING_PHRASE) {
          blockedByVideo = true;
          return t;
        }
        if (t.startsWith("Скачиваю модель")) {
          return t;
        }
        applied = true;
        return phrase;
      });
      // Always mark track (even if video won) so later UI doesn't flash music.
      musicAnnounceRef.current = phrase;
      if (applied && !blockedByVideo) {
        setBubbleLink(null);
        setBubbleVisible(true);
        // Hide is scheduled by the bubbleText effect (2s for music phrases).
      }
    }).then((fn) => unsubs.push(fn));

    // When distraction ends, return to music eyes if still playing.
    const musicEyeTimer = window.setInterval(() => {
      preferMusicEyes();
    }, 280);
    unsubs.push(() => window.clearInterval(musicEyeTimer));

    return () => unsubs.forEach((u) => u());
  }, []);

  async function copyExternalDigit() {
    if (!externalDigitOffer) return;
    const words = externalDigitOffer.words;
    try {
      if (isTauri()) {
        await invoke("digit_copy", { words });
      } else {
        await navigator.clipboard.writeText(words);
      }
      setExternalDigitOffer(null);
      setBubbleVisible(false);
      setMood("talk");
      emotionUntil.current = Date.now() + 1600;
      window.setTimeout(() => preferMusicEyes(), 1700);
    } catch (err) {
      console.warn("[mote] digit_copy failed", err);
      showBubble(words, null);
      scheduleHide(3500);
    }
  }

  async function replaceExternalDigit() {
    if (!externalDigitOffer) return;
    const words = externalDigitOffer.words;
    if (!isTauri()) {
      await copyExternalDigit();
      return;
    }
    try {
      await invoke("digit_replace", { words });
      setExternalDigitOffer(null);
      setBubbleVisible(false);
      setMood("talk");
      emotionUntil.current = Date.now() + 1200;
      // Keep current track marked announced — no «Слушаю» after replace.
      window.setTimeout(() => preferMusicEyes(), 1300);
    } catch (err) {
      console.warn("[mote] digit_replace failed", err);
      showBubble("Не удалось заменить — попробуй скопировать.", null);
      scheduleHide(3000);
    }
  }

  async function factExternalDigit() {
    if (!externalDigitOffer) return;
    const digits = externalDigitOffer.digits.replace(/\D/g, "");
    if (!digits || digits.length > 6) {
      showBubble("Для факта нужно число до 6 цифр.", null);
      scheduleHide(3500);
      return;
    }
    const n = Number(digits);
    setExternalDigitOffer(null);
    showBubble("…", null);
    setMood("curious");
    try {
      const line = await fetchNumberFact(n);
      if (
        !line.text ||
        line.text === "Ничего не нашлось." ||
        line.text === "…"
      ) {
        showBubble("Факт об этом числе не нашёлся.", null);
        scheduleHide(4000);
        return;
      }
      if (line.url) lastCuriousUrl.current = line.url;
      showBubble(line.text, line.url ?? null);
      scheduleHide(8000);
      emotionUntil.current = Date.now() + 4000;
    } catch {
      showBubble("Факт об этом числе не нашёлся.", null);
      scheduleHide(4000);
    }
  }

  async function copyExternalSpell() {
    if (!externalSpellOffer) return;
    // Prefer the full corrected sentence for paste elsewhere.
    const text =
      externalSpellOffer.replaced || externalSpellOffer.suggestion;
    let ok = false;
    try {
      // Win32 clipboard first — navigator.clipboard often fails in the overlay webview.
      if (isTauri()) {
        await invoke("digit_copy", { words: text });
        ok = true;
      }
    } catch (err) {
      console.warn("[mote] digit_copy failed", err);
    }
    if (!ok) {
      try {
        await navigator.clipboard.writeText(text);
        ok = true;
      } catch {
        /* show text below */
      }
    }
    if (ok) {
      spellOfferRef.current = false;
      setExternalSpellOffer(null);
      setBubbleVisible(false);
      setMood("talk");
      emotionUntil.current = Date.now() + 1600;
      window.setTimeout(() => preferMusicEyes(), 1700);
    } else {
      setExternalSpellOffer(null);
      spellOfferRef.current = false;
      showBubble(`Не скопировалось:\n${text}`, null);
      scheduleHide(5000);
    }
  }

  async function waitModelReady(timeoutMs = 600_000): Promise<boolean> {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      try {
        const st = await invoke<{
          modelPresent: boolean;
          runtimePresent: boolean;
        }>("llm_status");
        if (st.modelPresent && st.runtimePresent) return true;
      } catch {
        /* retry */
      }
      await new Promise((r) => window.setTimeout(r, 1500));
    }
    return false;
  }

  async function applyImprovedSpell(
    improved: string,
    source: string,
    word: string,
  ) {
    spellOfferRef.current = true;
    setExternalSpellOffer({
      word,
      suggestion: improved,
      replaced: improved,
      original: source,
    });
    setBubbleVisible(true);
    // Plaque often already shows the dict fix — write into the field so
    // «Улучшить» feels like it did something.
    try {
      await invoke("spell_replace", {
        replaced: improved,
        suggestion: improved,
      });
      spellOfferRef.current = false;
      setExternalSpellOffer(null);
      setSpellStatus(null);
      setBubbleVisible(false);
      setMood("talk");
      emotionUntil.current = Date.now() + 1200;
      window.setTimeout(() => preferMusicEyes(), 1300);
    } catch (err) {
      console.warn("[mote] improve apply failed", err);
      setSpellStatus("готово — нажми Заменить");
    }
  }

  async function improveExternalSpell() {
    if (!externalSpellOffer || !isTauri()) return;
    const offer = externalSpellOffer;
    // Prefer raw field text — `replaced` is already dictionary-fixed, so the
    // model would echo it and «Улучшить» looked broken.
    const source = (
      offer.original ||
      offer.replaced ||
      offer.suggestion
    ).trim();
    if (!source || source.length > 2000) {
      setSpellStatus("нечего улучшать");
      return;
    }
    setSpellStatus("Думаю…");
    setMood("curious");
    try {
      const res = await invoke<{ improved: string }>("llm_improve_text", {
        text: source,
      });
      const improved = (res.improved || "").trim();
      if (!improved) {
        setSpellStatus("пустой ответ модели");
        return;
      }
      await applyImprovedSpell(improved, source, offer.word);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes("DOWNLOADING")) {
        setSpellStatus("Скачиваю модель…");
        const ok = await waitModelReady();
        if (ok) {
          setSpellStatus("Думаю…");
          try {
            const res = await invoke<{ improved: string }>("llm_improve_text", {
              text: source,
            });
            const improved = (res.improved || "").trim();
            if (!improved) {
              setSpellStatus("пустой ответ модели");
              return;
            }
            await applyImprovedSpell(improved, source, offer.word);
            return;
          } catch (e2) {
            setSpellStatus(e2 instanceof Error ? e2.message : String(e2));
            return;
          }
        }
        setSpellStatus("скачивание не завершилось");
        return;
      }
      console.warn("[mote] llm_improve_text failed", err);
      setSpellStatus(msg || "не удалось улучшить");
    }
  }

  async function typografExternalSpell() {
    if (!externalSpellOffer || !isTauri()) return;
    const offer = externalSpellOffer;
    let source = (
      offer.original ||
      offer.replaced ||
      offer.suggestion
    ).trim();
    if (!source || source.length > MAX_TYPOGRAF_CHARS) {
      setSpellStatus("нечего типографить");
      return;
    }
    try {
      // Wrong EN layout → flip first, then typography (quotes/dashes/nbsp).
      const flipped = await invoke<string | null>("layout_fix", {
        text: source,
        force: false,
      });
      if (flipped && flipped.trim() && flipped !== source) {
        console.info("[mote] typograf via layout chars=%d", flipped.length);
        source = flipped.trim();
      }
      const out = applyTypograf(source);
      if (out == null) {
        setSpellStatus("нечего типографить");
        return;
      }
      if (out === source && flipped == null) {
        console.info("[mote] typograf noop chars=%d", source.length);
        setSpellStatus("уже ок");
        return;
      }
      console.info("[mote] typograf ok chars=%d", out.length);
      await applyImprovedSpell(out, offer.original || source, offer.word);
    } catch (err) {
      console.warn("[mote] typograf err", err);
      setSpellStatus(err instanceof Error ? err.message : "типограф сбой");
    }
  }

  async function layoutExternalSpell() {
    if (!externalSpellOffer || !isTauri()) return;
    const offer = externalSpellOffer;
    // Keep edges — Rust flip does not trim the payload.
    const source = offer.original || offer.replaced || offer.suggestion;
    if (!source.trim() || [...source].length > 2000) {
      setSpellStatus("нечего менять");
      return;
    }
    try {
      const out = await invoke<string | null>("layout_fix", {
        text: source,
        force: true,
      });
      if (out == null || out === source) {
        console.info("[mote] layout_fix noop chars=%d", [...source].length);
        setSpellStatus("уже ок");
        return;
      }
      console.info("[mote] layout_fix ok chars=%d", [...out].length);
      await applyImprovedSpell(out, source, offer.word);
    } catch (err) {
      console.warn("[mote] layout_fix err", err);
      setSpellStatus(err instanceof Error ? err.message : "раскладка сбой");
    }
  }

  async function keepExternalSpell() {
    if (!externalSpellOffer || !isTauri()) return;
    const word = externalSpellOffer.word.trim();
    if (!word) return;
    try {
      const n = await invoke<number>("spell_ignore_add", { word });
      console.info("[mote] spell_ignore add n=%d «%s»", n, word);
      spellOfferRef.current = false;
      setExternalSpellOffer(null);
      setSpellStatus(n > 0 ? "запомнил, больше не предлагаю" : "уже в списке");
      setBubbleVisible(true);
      window.setTimeout(() => {
        setSpellStatus(null);
        setBubbleVisible(false);
      }, 1600);
      void invoke("spell_clear_target");
    } catch (err) {
      console.warn("[mote] spell_ignore_add err", err);
      setSpellStatus(err instanceof Error ? err.message : "не запомнил");
    }
  }

  async function replaceExternalSpell() {
    if (!externalSpellOffer) return;
    const { suggestion, replaced } = externalSpellOffer;
    const text = replaced || suggestion;
    if (!isTauri()) {
      await copyExternalSpell();
      return;
    }
    try {
      await invoke("spell_replace", { replaced: text, suggestion });
      spellOfferRef.current = false;
      setExternalSpellOffer(null);
      setSpellStatus(null);
      setBubbleVisible(false);
      setMood("talk");
      emotionUntil.current = Date.now() + 1200;
      window.setTimeout(() => preferMusicEyes(), 1300);
    } catch (err) {
      console.warn("[mote] spell_replace failed", err);
      // Fall back: leave corrected text on the Win32 clipboard.
      try {
        await invoke("digit_copy", { words: text });
      } catch {
        try {
          await navigator.clipboard.writeText(text);
        } catch {
          /* ignore */
        }
      }
      setBubbleVisible(true);
      setMood("curious");
    }
  }

  // Live idle only when not chatting and not following cursor/typing.
  useEffect(() => {
    if (busy || editing) return;

    let cancelled = false;
    const timers: number[] = [];

    const later = (ms: number, fn: () => void) => {
      const id = window.setTimeout(() => {
        if (!cancelled) fn();
      }, ms);
      timers.push(id);
    };

    const tick = () => {
      later(rand(1800, 5200), () => {
        if (busyRef.current || editingRef.current) return;
        if (Date.now() < followUntil.current || Date.now() < emotionUntil.current) {
          tick();
          return;
        }
        if (musicPlayingRef.current) {
          setMood("music");
          tick();
          return;
        }

        const action = pickIdle();
        setMood((m) => (isIdleMood(m) ? action : m));

        // Rare idle curiosity: wiki, or quote only if today's quote not shown yet.
        if (
          isTauri() &&
          Math.random() < 0.08 &&
          Date.now() - lastCuriousIdleAt.current > 12 * 60_000 &&
          !digitOfferRef.current &&
          !spellOfferRef.current
        ) {
          lastCuriousIdleAt.current = Date.now();
          const quoteKey = `mote-quote-day-${new Date().toDateString()}`;
          const quotePending = !localStorage.getItem(quoteKey);
          void (quotePending && Math.random() < 0.55
            ? fetchQuote().then((line) => {
                localStorage.setItem(quoteKey, "1");
                return line;
              })
            : fetchWikiRandom()
          )
            .then((line) => {
              if (busyRef.current || editingRef.current) return;
              lastCuriousUrl.current = line.url ?? null;
              showBubble(line.text, line.url ?? null);
              setMood("curious");
              emotionUntil.current = Date.now() + 4500;
              scheduleHide(8000);
            })
            .catch(() => {});
        }

        later(rand(900, 2400), () => {
          if (busyRef.current || editingRef.current) return;
          if (Date.now() < followUntil.current || Date.now() < emotionUntil.current) {
            tick();
            return;
          }
          if (musicPlayingRef.current) {
            setMood("music");
            tick();
            return;
          }
          setMood((m) => (isIdleMood(m) ? "idle" : m));
          tick();
        });
      });
    };

    tick();

    return () => {
      cancelled = true;
      timers.forEach((id) => window.clearTimeout(id));
    };
  }, [busy, editing]);

  /** Show plaque text; pass link only for curious results (wiki/quote/…). */
  function showBubble(text: string, link: string | null = null) {
    setBubbleText(text);
    setBubbleLink(link);
    setBubbleVisible(true);
  }

  function scheduleHide(ms = 10000) {
    if (hideTimer.current) window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => {
      if (!editingRef.current && !busyRef.current) setBubbleVisible(false);
    }, ms);
  }

  useEffect(() => {
    if (!bubbleVisible || editing || busy || externalDigitOffer || externalSpellOffer) return;
    // Keep progress visible until ready/error — don't fight SMTC with a hide timer.
    if (bubbleText.startsWith("Скачиваю модель")) return;
    // Music plaque: brief flash. Other bubbles: longer idle hide.
    // Must reschedule here — cleanup below clears any prior timer.
    if (isMusicPhrase(bubbleText)) {
      scheduleHide(2000);
    } else if (bubbleText !== WATCHING_PHRASE) {
      scheduleHide();
    }
    return () => {
      if (hideTimer.current) window.clearTimeout(hideTimer.current);
    };
  }, [bubbleText, bubbleVisible, editing, busy, externalDigitOffer, externalSpellOffer]);

  function setPresencePaused(paused: boolean) {
    if (!isTauri()) return;
    void invoke("set_presence_paused", { paused });
  }

  /** Stop caret-dock while the user is pressing the pet (esp. glasses / type mode). */
  function onPetPointerGesture(active: boolean) {
    if (active) {
      // Fresh press — don't let a previous drag permanently block clicks.
      dragMoved.current = false;
      setPresencePaused(true);
      return;
    }
    if (!editingRef.current && !busyRef.current) {
      setPresencePaused(false);
    }
  }

  function startDrag() {
    if (!isTauri()) return;
    // Mark as drag so the click that ends the gesture is ignored once.
    dragMoved.current = true;
    // Pause auto-dock immediately; startDragging must stay sync with the gesture.
    setPresencePaused(true);
    const win = getCurrentWindow();
    void win
      .startDragging()
      .then(() => {
        // Keep the place the user put Mote — don't snap back to video/input.
        void invoke("mark_user_placed", { durationMs: 3_500 });
      })
      .catch(() => {
        // Drag cancelled / failed — allow next click.
        dragMoved.current = false;
      })
      .finally(() => {
        if (!editingRef.current && !busyRef.current) {
          setPresencePaused(false);
        }
      });
  }

  function openCompose() {
    if (busy) return;
    if (hideTimer.current) window.clearTimeout(hideTimer.current);
    setMenu(false);
    setBubbleVisible(true);
    setEditing(true);
  }

  /** Close compose UI but keep draft text in `input`. */
  function dismissComposeKeepDraft() {
    setEditing(false);
    setBubbleVisible(false);
    setMenu(false);
  }

  function onPetClick() {
    if (dragMoved.current) {
      dragMoved.current = false;
      return;
    }
    openCompose();
  }

  async function enterRage() {
    setMenu(false);
    setVolumePanelOpen(false);
    setEditing(false);
    setBubbleVisible(false);
    setExternalDigitOffer(null);
    setExternalSpellOffer(null);
    setMood("angry");
    emotionUntil.current = Date.now() + 60_000;
    setRaging(true);
    if (!isTauri()) return;
    try {
      await invoke("enter_rage_mode");
    } catch (err) {
      console.error("enter_rage_mode failed", err);
    }
  }

  async function forgiveRage() {
    shooCount.current = 0;
    shooTimes.current = [];
    if (isTauri()) {
      try {
        await invoke("exit_rage_mode");
      } catch (err) {
        console.error("exit_rage_mode failed", err);
      }
    }
    setRaging(false);
    setMood("offended");
    emotionUntil.current = Date.now() + 3600;
    showBubble("Ладно… прощаю. Но больше так не надо.", null);
    scheduleHide(3200);
    window.setTimeout(() => preferMusicEyes(), 3800);
  }

  async function onPetDoubleClick() {
    if (dragMoved.current) {
      dragMoved.current = false;
      return;
    }
    if (busyRef.current || editingRef.current || ragingRef.current) return;
    setMenu(false);

    // Drop glasses lock so caret dock / type heartbeats don't yank him back.
    glassesUntil.current = 0;
    typingUntil.current = 0;
    followUntil.current = 0;

    const now = Date.now();
    const windowMs = 50_000;
    shooTimes.current = shooTimes.current.filter((t) => now - t < windowMs);
    shooTimes.current.push(now);
    shooCount.current += 1;
    const recent = shooTimes.current.length;
    const total = shooCount.current;

    if (total >= 5) {
      await enterRage();
      return;
    }

    let nextMood: PetMood = "offended";
    let phrase = "Ой — ухожу.";
    if (recent >= 4 || total >= 4) {
      nextMood = "angry";
      phrase = "Хватит меня гонять!";
    } else if (recent >= 2 || total >= 2) {
      nextMood = "cry";
      phrase = "Опять… за что?";
    }

    setMood(nextMood);
    emotionUntil.current = now + (nextMood === "angry" ? 4200 : 3200);
    showBubble(phrase, null);
    scheduleHide(nextMood === "angry" ? 3200 : 2400);

    if (!isTauri()) return;
    try {
      await invoke("shoo_away");
      dragMoved.current = false;
    } catch (err) {
      console.error("shoo_away failed", err);
      showBubble("Не смог улететь…", null);
      scheduleHide(2500);
    }
    const hold = emotionUntil.current - Date.now();
    window.setTimeout(() => preferMusicEyes(), Math.max(50, hold + 50));
  }

  function onPetContextMenu(e: ReactMouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    setVolumePanelOpen(false);
    setMenu((open) => !open);
  }

  async function mediaAction(action: string) {
    // Keep the side menu open while skipping / pausing / resuming.
    if (!isTauri()) return;
    try {
      await invoke("media_control", { action });
    } catch {
      setBubbleVisible(true);
      showBubble("Не удалось управлять музыкой.", null);
      scheduleHide(3500);
    }
  }

  // Click outside / leave window while composing → hide plaque, keep draft.
  useEffect(() => {
    if (!editing) return;

    const onPointerDown = (e: PointerEvent) => {
      const t = e.target as HTMLElement | null;
      if (!t) return;
      if (t.closest(".speech, .pet-avatar, .mote-menu")) return;
      dismissComposeKeepDraft();
    };
    const onBlur = () => {
      dismissComposeKeepDraft();
    };

    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("blur", onBlur);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("blur", onBlur);
    };
  }, [editing]);

  // Close context menu on outside click / Escape.
  useEffect(() => {
    if (!menu) return;
    const closeMenu = () => {
      setVolumePanelOpen(false);
      setMenu(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeMenu();
    };
    const onDown = (e: PointerEvent) => {
      const t = e.target as HTMLElement | null;
      if (t?.closest(".mote-menu")) return;
      // ПКМ по питомцу сам переключает меню — не гасим заранее.
      if (t?.closest(".pet-avatar") && e.button === 2) return;
      closeMenu();
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("pointerdown", onDown, true);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("pointerdown", onDown, true);
    };
  }, [menu]);

  async function send() {
    const text = input.trim();
    if (!text || busy) return;

    const userMsg = createMessage("user", text);
    const nextHistory = [...messages, userMsg];
    setMessages(nextHistory);
    setInput("");
    setEditing(false);
    setBusy(true);
    setMood("think");
    emotionUntil.current = Date.now() + 8000;
    setBubbleVisible(true);
    setBubbleText("…");

    try {
      let reply: string;
      let replyLink: string | null = null;
      if (/^(открой|открыть|ссылка|подробнее)$/i.test(text) && lastCuriousUrl.current) {
        try {
          await openCuriousUrl(lastCuriousUrl.current);
          reply = "Открыл в браузере.";
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          reply = `Не открылось: ${msg}`;
        }
      } else if (isWeatherAsk(text)) {
        const city = cityFromWeatherAsk(text) ?? settings.weatherCity;
        try {
          reply = await fetchWeather(city);
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          reply = `Не достал погоду: ${msg}`;
        }
      } else {
        const curious = await tryCuriousAsk(text);
        if (curious) {
          reply = curious.text;
          replyLink = curious.url ?? null;
          if (replyLink) lastCuriousUrl.current = replyLink;
        } else {
          reply = await engine.ask(text, nextHistory, settings.petName);
        }
      }
      const nextMood = detectMood(text, reply);
      setMood(nextMood);
      showBubble(reply, replyLink);
      setMessages((prev) => [...prev, createMessage("assistant", reply)]);
      const hold = nextMood === "talk" ? 1600 : 3600;
      emotionUntil.current = Date.now() + hold;
      window.setTimeout(() => {
        setMood((m) => {
          if (m !== nextMood) return m;
          return musicPlayingRef.current ? "music" : "idle";
        });
      }, hold);
    } catch {
      const msg = "Ничего не нашлось.";
      showBubble(msg, null);
      setMessages((prev) => [...prev, createMessage("assistant", msg)]);
      setMood("offended");
      emotionUntil.current = Date.now() + 3200;
      window.setTimeout(() => {
        setMood((m) => {
          if (m !== "offended") return m;
          return musicPlayingRef.current ? "music" : "idle";
        });
      }, 3200);
    } finally {
      setBusy(false);
    }
  }

  async function openBubbleLink() {
    const url = bubbleLink;
    if (!url) return;
    try {
      await openCuriousUrl(url);
    } catch {
      showBubble("Ничего не нашлось.", null);
    }
  }

  if (raging) {
    return (
      <div className="shell shell-rage">
        <RageOverlay petName={settings.petName} onForgive={() => void forgiveRage()} />
      </div>
    );
  }

  return (
    <div className={`shell${musicPlaying ? " shell-music" : ""}`}>
      <SpeechBubble
        text={bubbleText}
        editing={editing}
        input={input}
        busy={busy}
        visible={bubbleVisible}
        linkUrl={bubbleLink}
        onOpenLink={() => void openBubbleLink()}
        externalDigitOffer={editing ? null : externalDigitOffer}
        externalSpellOffer={editing ? null : externalSpellOffer}
        onOpenEdit={openCompose}
        onInputChange={setInput}
        onSubmit={() => void send()}
        onCancel={dismissComposeKeepDraft}
        onCopyExternalDigit={() => void copyExternalDigit()}
        onReplaceExternalDigit={() => void replaceExternalDigit()}
        onFactExternalDigit={() => void factExternalDigit()}
        onDismissExternalDigit={() => dismissDigitOfferUi()}
        onCopyExternalSpell={() => void copyExternalSpell()}
        onReplaceExternalSpell={() => void replaceExternalSpell()}
        onImproveExternalSpell={() => void improveExternalSpell()}
        onTypografExternalSpell={() => void typografExternalSpell()}
        onLayoutExternalSpell={() => void layoutExternalSpell()}
        onKeepExternalSpell={() => void keepExternalSpell()}
        spellStatus={spellStatus}
        onDismissExternalSpell={() => {
          spellOfferRef.current = false;
          setExternalSpellOffer(null);
          setSpellStatus(null);
          setBubbleLink(null);
          setBubbleText(GREETING);
          setBubbleVisible(false);
          if (isTauri()) void invoke("spell_clear_target");
        }}
      />
      <PetAvatar
        mood={mood}
        name={settings.petName}
        lookX={look.x}
        lookY={look.y}
        eyesTracking={eyesTracking}
        musicPlaying={musicPlaying}
        onPointerGesture={onPetPointerGesture}
        onDragStart={startDrag}
        onClick={onPetClick}
        onDoubleClick={() => void onPetDoubleClick()}
        onContextMenu={onPetContextMenu}
      />
      {menu && (
        <ContextMenu
          musicPlaying={musicPlaying}
          onToggleMusic={() => void mediaAction(musicPlaying ? "pause" : "play")}
          onPreviousTrack={() => void mediaAction("previous")}
          onNextTrack={() => void mediaAction("next")}
          onVolumePanelChange={setVolumePanelOpen}
        />
      )}
    </div>
  );
}
