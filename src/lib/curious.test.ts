import { describe, expect, it } from "vitest";
import { parseCuriousIntent, shouldShowGoLink } from "./curious";

describe("parseCuriousIntent", () => {
  it("parses wiki prefixes and ?", () => {
    expect(parseCuriousIntent("вики путин")).toEqual({ kind: "wiki", query: "путин" });
    expect(parseCuriousIntent("? из чего состоит пластик")).toEqual({
      kind: "wiki",
      query: "из чего состоит пластик",
    });
    expect(parseCuriousIntent("вики")).toEqual({ kind: "wiki", query: null });
    expect(parseCuriousIntent("?")).toEqual({ kind: "wiki", query: null });
    expect(parseCuriousIntent("wiki Einstein")).toEqual({ kind: "wiki", query: "Einstein" });
  });

  it("parses other short commands", () => {
    expect(parseCuriousIntent("космос")).toEqual({ kind: "nasa" });
    expect(parseCuriousIntent("цитата")).toEqual({ kind: "quote" });
    expect(parseCuriousIntent("столица франция")).toEqual({
      kind: "country",
      name: "франция",
    });
    expect(parseCuriousIntent("книга dune")).toEqual({ kind: "book", query: "dune" });
    expect(parseCuriousIntent("факт 42")).toEqual({ kind: "number", n: 42 });
    expect(parseCuriousIntent("факт дня")).toEqual({ kind: "date" });
  });

  it("ignores plain chat", () => {
    expect(parseCuriousIntent("привет")).toBeNull();
    expect(parseCuriousIntent("что умеешь")).toBeNull();
    expect(parseCuriousIntent("погода")).toBeNull();
  });
});

describe("shouldShowGoLink", () => {
  const wiki = "https://ru.wikipedia.org/wiki/Пластмасса";

  it("shows arrow only for curious results with http url", () => {
    expect(shouldShowGoLink("Пластмасса: полимеры…", wiki)).toBe(true);
    expect(shouldShowGoLink("Кстати: 42 is…", "http://numbersapi.com/42")).toBe(true);
  });

  it("hides arrow on shoo / music / empty / not found", () => {
    expect(shouldShowGoLink("Опять… за что?", wiki)).toBe(false);
    expect(shouldShowGoLink("Ой — ухожу.", wiki)).toBe(false);
    expect(shouldShowGoLink("Хватит меня гонять!", wiki)).toBe(false);
    expect(shouldShowGoLink("Слушаю: Artist — Track", wiki)).toBe(false);
    expect(shouldShowGoLink("Смотрю рядом.", wiki)).toBe(false);
    expect(shouldShowGoLink("Ничего не нашлось.", wiki)).toBe(false);
    expect(shouldShowGoLink("Пластмасса: …", null)).toBe(false);
    expect(shouldShowGoLink("Пластмасса: …", "not-a-url")).toBe(false);
  });
});
