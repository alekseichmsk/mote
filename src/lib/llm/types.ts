import { ChatMessage } from "../types";

export interface LlmProvider {
  readonly id: "mock" | "deepseek";
  reply(input: string, history: ChatMessage[], petName: string): Promise<string>;
}
