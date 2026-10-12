import { ChatMessage } from "../types";
import { DeepSeekProvider } from "./deepseekProvider";
import { MockProvider } from "./mockProvider";
import { LlmProvider } from "./types";

export class ChatEngine {
  private provider: LlmProvider;

  constructor(provider: LlmProvider = new MockProvider()) {
    this.provider = provider;
  }

  setProvider(kind: "mock" | "deepseek", apiKey = "") {
    this.provider = kind === "deepseek" ? new DeepSeekProvider(apiKey) : new MockProvider();
  }

  async ask(input: string, history: ChatMessage[], petName: string): Promise<string> {
    return this.provider.reply(input, history, petName);
  }
}

export function createMessage(role: ChatMessage["role"], content: string): ChatMessage {
  return {
    id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    role,
    content,
    createdAt: Date.now(),
  };
}
