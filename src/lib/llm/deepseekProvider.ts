import { LlmProvider } from "./types";
import { ChatMessage } from "../types";

/** Stub for a later release. Does not call the network in MVP. */
export class DeepSeekProvider implements LlmProvider {
  readonly id = "deepseek" as const;

  constructor(private readonly apiKey: string) {}

  async reply(_input: string, _history: ChatMessage[], petName: string): Promise<string> {
    void this.apiKey;
    return `${petName}: DeepSeek ещё не подключён в MVP. Переключись на mock или дождись следующей версии.`;
  }
}
