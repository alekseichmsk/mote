import { FormEvent, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { loadSettings, saveSettings } from "./lib/storage";
import { DEFAULT_SETTINGS, MoteSettings } from "./lib/types";

type Tab = "main" | "commands";

type LlmDevice = "gpu" | "cpu";

type LlmStatus = {
  ready: boolean;
  modelPath: string;
  modelBytes: number;
  modelPresent: boolean;
  runtimePresent: boolean;
  runtimePath: string;
  sidecarRunning: boolean;
  port: number;
  detail: string;
  device: LlmDevice;
  gpuRuntimePresent: boolean;
  cpuRuntimePresent: boolean;
};

const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const COMMANDS: { cmd: string; desc: string }[] = [
  { cmd: "? … · вики …", desc: "Википедия" },
  { cmd: "вики", desc: "случайная статья" },
  { cmd: "погода", desc: "погода (город во вкладке Основное)" },
  { cmd: "столица … · страна …", desc: "страна" },
  { cmd: "книга …", desc: "книга" },
  { cmd: "космос", desc: "фото дня NASA" },
  { cmd: "цитата", desc: "цитата" },
  { cmd: "факт 42", desc: "факт о числе" },
  { cmd: "факт дня", desc: "факт о сегодняшней дате" },
  { cmd: "открой", desc: "последняя ссылка из ответа" },
  { cmd: "ПКМ по Mote", desc: "музыка / громкость" },
  { cmd: "Ctrl+Shift+M", desc: "показать / скрыть" },
  { cmd: "Ctrl+Shift+S", desc: "орфография выделенного текста" },
  { cmd: "Улучшить (plaque)", desc: "локальная LLM, смысл и запятые" },
  { cmd: "Авто-раскладка", desc: "после пробела: ghbdtn → привет (флаг в Основном)" },
];

export default function SettingsApp() {
  const [settings, setSettings] = useState<MoteSettings>(() => loadSettings());
  const [savedFlash, setSavedFlash] = useState(false);
  const [tab, setTab] = useState<Tab>("main");
  const [llm, setLlm] = useState<LlmStatus | null>(null);
  const [llmProgress, setLlmProgress] = useState<string | null>(null);
  const [llmBusy, setLlmBusy] = useState(false);

  useEffect(() => {
    document.title = "Mote — настройки";
    document.documentElement.classList.add("settings-root");
    document.body.classList.add("settings-root");
    return () => {
      document.documentElement.classList.remove("settings-root");
      document.body.classList.remove("settings-root");
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        void getCurrentWindow().close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<MoteSettings>("mote://settings-changed", (event) => {
      setSettings({ ...DEFAULT_SETTINGS, ...event.payload });
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    void refreshLlm();
    void invoke<boolean>("layout_auto_get")
      .then((on) => {
        setSettings((prev) =>
          prev.layoutAuto === on ? prev : { ...prev, layoutAuto: on },
        );
      })
      .catch(() => {
        /* older binary */
      });
    let unlisten: (() => void) | undefined;
    void listen<{ phase: string; percent: number; detail: string }>(
      "mote://model-progress",
      (event) => {
        const { phase, percent, detail } = event.payload;
        if (phase === "download") {
          setLlmProgress(`${percent}% — ${detail}`);
          setLlmBusy(true);
        } else if (phase === "ready") {
          setLlmProgress(detail);
          setLlmBusy(false);
          void refreshLlm();
        } else if (phase === "error") {
          setLlmProgress(detail);
          setLlmBusy(false);
        }
      },
    ).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  async function refreshLlm() {
    if (!isTauri()) return;
    try {
      const st = await invoke<LlmStatus>("llm_status");
      setLlm(st);
      setSettings((prev) =>
        prev.llmDevice === st.device ? prev : { ...prev, llmDevice: st.device },
      );
    } catch {
      setLlm(null);
    }
  }

  async function downloadModel() {
    if (!isTauri()) return;
    setLlmBusy(true);
    setLlmProgress("старт…");
    try {
      // Apply selected device before download so the right engine is fetched.
      await invoke("llm_set_device", { device: settings.llmDevice });
      saveSettings({ ...settings, llmDevice: settings.llmDevice });
      await invoke("llm_download_model");
    } catch (e) {
      setLlmProgress(e instanceof Error ? e.message : String(e));
      setLlmBusy(false);
    }
  }

  function patch<K extends keyof MoteSettings>(key: K, value: MoteSettings[K]) {
    setSettings((prev) => ({ ...prev, [key]: value }));
  }

  async function persist(next: MoteSettings) {
    const cleaned: MoteSettings = {
      ...next,
      petName: next.petName.trim() || DEFAULT_SETTINGS.petName,
      weatherCity: next.weatherCity.trim() || DEFAULT_SETTINGS.weatherCity,
      llmDevice: next.llmDevice === "cpu" ? "cpu" : "gpu",
      layoutAuto: !!next.layoutAuto,
    };
    if (isTauri()) {
      try {
        await invoke("llm_set_device", { device: cleaned.llmDevice });
        await invoke("layout_auto_set", { enabled: cleaned.layoutAuto });
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setLlmProgress(`не удалось сохранить: ${msg}`);
        return;
      }
    }
    saveSettings(cleaned);
    setSettings(cleaned);
    try {
      await emit("mote://settings-changed", cleaned);
    } catch {
      /* browser preview */
    }
    void refreshLlm();
    setSavedFlash(true);
    window.setTimeout(() => setSavedFlash(false), 1200);
  }

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    void persist(settings);
  }

  async function closeWindow() {
    await persist(settings);
    try {
      await getCurrentWindow().close();
    } catch {
      /* ignore */
    }
  }

  const modelReady =
    !!llm?.modelPresent &&
    (settings.llmDevice === "gpu" ? !!llm.gpuRuntimePresent : !!llm.cpuRuntimePresent);

  return (
    <div className="settings-shell">
      <div className="settings-chrome" data-tauri-drag-region>
        <span className="settings-chrome-label" data-tauri-drag-region>
          Настройки
        </span>
        <button
          type="button"
          className="settings-chrome-close"
          aria-label="Закрыть"
          onClick={() => void closeWindow()}
        >
          ×
        </button>
      </div>
      <header className="settings-header" data-tauri-drag-region>
        <div className="settings-header-text" data-tauri-drag-region>
          <p className="settings-brand">Mote</p>
          <h1 className="settings-title">Настройки</h1>
          <p className="settings-sub">питомец · текст · локальная модель</p>
        </div>
        <div className="settings-lamp" aria-hidden />
      </header>

      <nav className="settings-tabs" aria-label="Разделы">
        <button
          type="button"
          className={`settings-tab${tab === "main" ? " settings-tab-active" : ""}`}
          onClick={() => setTab("main")}
        >
          Основное
        </button>
        <button
          type="button"
          className={`settings-tab${tab === "commands" ? " settings-tab-active" : ""}`}
          onClick={() => setTab("commands")}
        >
          Команды
        </button>
      </nav>

      {tab === "main" ? (
        <form className="settings-form" onSubmit={onSubmit}>
          <section className="settings-card">
            <h2 className="settings-card-title">Питомец</h2>
            <label className="settings-field">
              <span>Имя</span>
              <input
                value={settings.petName}
                onChange={(e) => patch("petName", e.target.value)}
                maxLength={32}
                autoComplete="off"
              />
            </label>
            <label className="settings-field">
              <span>Город для погоды</span>
              <input
                value={settings.weatherCity}
                onChange={(e) => patch("weatherCity", e.target.value)}
                placeholder="Moscow"
                autoComplete="off"
                spellCheck={false}
              />
            </label>
          </section>

          <section className="settings-card">
            <h2 className="settings-card-title">Текст</h2>
            <label className="settings-toggle">
              <input
                type="checkbox"
                checked={!!settings.layoutAuto}
                onChange={(e) => patch("layoutAuto", e.target.checked)}
              />
              <span className="settings-toggle-ui" aria-hidden />
              <span className="settings-toggle-copy">
                <span className="settings-toggle-label">Авто-раскладка</span>
                <span className="settings-toggle-hint">
                  Латиница → русский после пробела. EN-слова, CAPS и ссылки не трогает.
                </span>
              </span>
            </label>
          </section>

          <section className="settings-card">
            <h2 className="settings-card-title">Локальная модель</h2>
            <div className="settings-llm-device" role="radiogroup" aria-label="Устройство нейросети">
              <label
                className={`settings-chip${settings.llmDevice === "gpu" ? " settings-chip-on" : ""}`}
              >
                <input
                  type="radio"
                  name="llmDevice"
                  checked={settings.llmDevice === "gpu"}
                  onChange={() => patch("llmDevice", "gpu")}
                />
                <span>GPU</span>
              </label>
              <label
                className={`settings-chip${settings.llmDevice === "cpu" ? " settings-chip-on" : ""}`}
              >
                <input
                  type="radio"
                  name="llmDevice"
                  checked={settings.llmDevice === "cpu"}
                  onChange={() => patch("llmDevice", "cpu")}
                />
                <span>CPU</span>
              </label>
            </div>
            <p className="settings-llm-detail">
              {llm
                ? [
                    settings.llmDevice === "gpu" ? "режим GPU" : "режим CPU",
                    llm.modelPresent
                      ? `модель ${(llm.modelBytes / 1e9).toFixed(1)} ГБ`
                      : "модель не скачана (~2 ГБ)",
                    settings.llmDevice === "gpu"
                      ? llm.gpuRuntimePresent
                        ? "CUDA ок"
                        : "CUDA не скачан (~600 МБ)"
                      : llm.cpuRuntimePresent
                        ? "CPU-движок ок"
                        : "CPU-движок не скачан (~20 МБ)",
                    llm.sidecarRunning
                      ? `sidecar :${llm.port}`
                      : modelReady
                        ? "ждёт «Улучшить»"
                        : null,
                  ]
                    .filter(Boolean)
                    .join(" · ")
                : "статус недоступен"}
            </p>
            {llmProgress ? <p className="settings-llm-progress">{llmProgress}</p> : null}
            <div className="settings-llm-actions">
              <button
                type="button"
                className="settings-btn"
                disabled={llmBusy || modelReady}
                onClick={() => void downloadModel()}
              >
                {modelReady ? "Всё на месте" : "Скачать недостающее"}
              </button>
              <button
                type="button"
                className="settings-btn"
                onClick={() => void refreshLlm()}
              >
                Обновить
              </button>
            </div>
            <p className="settings-llm-hint">
              GPU — NVIDIA. После «Сохранить» sidecar перезапустится при следующем Улучшить.
            </p>
          </section>

          <div className="settings-actions">
            <button type="submit" className="settings-btn settings-btn-main">
              {savedFlash ? "Сохранено" : "Сохранить"}
            </button>
            <button type="button" className="settings-btn" onClick={() => void closeWindow()}>
              Готово
            </button>
          </div>
        </form>
      ) : (
        <div className="settings-commands">
          <section className="settings-card settings-card-commands">
            <h2 className="settings-card-title">Что можно сказать</h2>
            <ul className="settings-cmd-list">
              {COMMANDS.map((row) => (
                <li key={row.cmd} className="settings-cmd-row">
                  <code className="settings-cmd">{row.cmd}</code>
                  <span className="settings-cmd-desc">{row.desc}</span>
                </li>
              ))}
            </ul>
          </section>
          <div className="settings-actions">
            <button type="button" className="settings-btn" onClick={() => void closeWindow()}>
              Готово
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
