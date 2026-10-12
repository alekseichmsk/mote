export type PetMood =
  | "idle"
  | "glanceLeft"
  | "glanceRight"
  | "glanceUp"
  | "glanceDown"
  | "curious"
  | "drowsy"
  | "peek"
  | "music"
  | "focus"
  | "talk"
  | "think"
  | "angry"
  | "laugh"
  | "cry"
  | "offended";

export const IDLE_MOODS: PetMood[] = [
  "idle",
  "glanceLeft",
  "glanceRight",
  "glanceUp",
  "glanceDown",
  "curious",
  "drowsy",
  "peek",
  "music",
];

export function isIdleMood(mood: PetMood): boolean {
  return IDLE_MOODS.includes(mood);
}

export function moodFromLookDir(dir: string): PetMood {
  switch (dir) {
    case "left":
      return "glanceLeft";
    case "right":
      return "glanceRight";
    case "up":
      return "glanceUp";
    case "down":
      return "glanceDown";
    default:
      return "curious";
  }
}

export type ChatRole = "user" | "assistant" | "system";

export interface ChatMessage {
  id: string;
  role: ChatRole;
  content: string;
  createdAt: number;
}

export interface MoteSettings {
  petName: string;
  alwaysOnTop: boolean;
  autostart: boolean;
  deepseekApiKey: string;
  provider: "mock" | "deepseek";
  /** City for wttr.in, e.g. Moscow / Москва / Saint-Petersburg. */
  weatherCity: string;
  /** Local LLM compute: CUDA GPU or CPU. */
  llmDevice: "gpu" | "cpu";
  /** Live EN→RU after space/Enter when mistyped on wrong layout. */
  layoutAuto: boolean;
}

export const DEFAULT_SETTINGS: MoteSettings = {
  petName: "Mote",
  alwaysOnTop: true,
  autostart: false,
  deepseekApiKey: "",
  provider: "mock",
  weatherCity: "Moscow",
  llmDevice: "gpu",
  layoutAuto: false,
};

/** Pick an eye mood from user text + reply. Eyes only. */
export function detectMood(userInput: string, reply = ""): PetMood {
  const t = `${userInput}\n${reply}`.toLowerCase();

  if (/(зл|бесит|дурак|ненавиж|ярость|angry|rage|бешен)/i.test(t)) return "angry";
  if (/(ха+|аха|шутк|смех|смешн|lol|haha|joke|весел)/i.test(t)) return "laugh";
  if (/(плач|груст|слез|жалк|печал|cry|sad|тоск)/i.test(t)) return "cry";
  if (/(обид|прости|игнор|преда|равнодуш|offend|hurt)/i.test(t)) return "offended";
  if (/(дума|мысл|почему|как\b|think)/i.test(userInput)) return "think";

  return "talk";
}
