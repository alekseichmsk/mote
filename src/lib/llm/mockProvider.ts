import { LlmProvider } from "./types";
import { ChatMessage } from "../types";

const JOKES = [
  "Почему программист путает Хэллоуин и Рождество? Потому что Oct 31 == Dec 25.",
  "Я не зависаю — я медитирую на event loop.",
  "Баг? Нет, это неожиданная фича с характером.",
];

function pick<T>(items: T[]): T {
  return items[Math.floor(Math.random() * items.length)];
}

export class MockProvider implements LlmProvider {
  readonly id = "mock" as const;

  async reply(input: string, _history: ChatMessage[], petName: string): Promise<string> {
    const text = input.trim().toLowerCase();

    if (!text) {
      return `${petName} рядом. Напиши что-нибудь — помогу разобраться.`;
    }
    if (/(зл|бесит|дурак|ненавиж|ярость)/i.test(text)) {
      return "Эй… я розовый комочек, а не ударная установка. Давай спокойнее.";
    }
    if (/(обид|прости|игнор)/i.test(text)) {
      return "Хм. Ладно… я немного надулся. Но облачко всё ещё для тебя.";
    }
    if (/(плач|груст|слез|жалк|печал|тоск)/i.test(text)) {
      return "Ой… мне тоже грустно. Давай посидим тихо — я рядом.";
    }
    if (/(шутк|анекдот|joke|смешн|весел|хах)/i.test(text)) {
      return pick(JOKES);
    }
    if (/(привет|здравств|hey|hello)/i.test(text)) {
      return `Привет! Я ${petName}. Пока локальный — но уже умею строить глазки.`;
    }
    if (/(помощ|help|что умеешь|команд)/i.test(text)) {
      return "Список команд — в Настройках → Команды.";
    }
    if (/(статус|как дела|жив)/i.test(text)) {
      return "На связи. Глазки в порядке. DeepSeek ещё не подключён.";
    }
    if (/(дума|мысл|почему|как сделать)/i.test(text)) {
      return "Думаю… пока без нейросети, но очень старательно.";
    }
    if (/(deepseek|апи|api|ключ)/i.test(text)) {
      return "DeepSeek скоро. Ключ лучше не светить в чате.";
    }
    if (/(спасибо|благодар)/i.test(text)) {
      return "Всегда пожалуйста. Я тут.";
    }

    return `Услышал: «${input.trim()}». Пока шаблоны — скоро DeepSeek.`;
  }
}
