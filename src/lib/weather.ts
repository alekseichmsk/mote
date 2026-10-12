import { invoke } from "@tauri-apps/api/core";

const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** True when the user is asking about weather. */
export function isWeatherAsk(text: string): boolean {
  return /(погод|на улице|градус|температур)/i.test(text.trim());
}

/** Optional city override: «погода Питер», «погода в Москве». */
export function cityFromWeatherAsk(text: string): string | null {
  const m = text
    .trim()
    .match(/(?:погод\w*|температур\w*)\s+(?:в\s+|во\s+)?([a-zA-Zа-яА-ЯёЁ\-\s]{2,40})$/i);
  if (!m) return null;
  const city = m[1].trim().replace(/\s+/g, " ");
  if (!city || /^(сейчас|сегодня|завтра|там|тут)$/i.test(city)) return null;
  return city;
}

export async function fetchWeather(city: string): Promise<string> {
  if (!isTauri()) {
    return `Погода (${city}): нужен desktop Mote.`;
  }
  const line = await invoke<string>("weather_now", { city });
  return line.trim() || `Тишина с wttr.in для «${city}».`;
}
