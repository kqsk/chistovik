import { afterEach, describe, expect, it } from "vitest";
import { en } from "./locales/en";
import { ru } from "./locales/ru";
import { S, setLang } from "./strings";

describe("strings", () => {
  afterEach(() => setLang("ru"));

  it("switches language", () => {
    expect(S.tabGeneral).toBe("Общие");
    setLang("en");
    expect(S.tabGeneral).toBe("General");
  });

  it("English has every key and keeps placeholders", () => {
    for (const k of Object.keys(ru) as (keyof typeof ru)[]) {
      expect(en[k], k).toBeTruthy();
      for (const ph of ru[k].match(/\{\{?\w+\}?\}/g) ?? []) {
        expect(en[k], `${k} ${ph}`).toContain(ph);
      }
    }
  });
});
