import { describe, expect, it } from "vitest";
import { validateForm } from "./validate";

const ok = {
  name: "OpenAI",
  baseUrl: "https://a/v1",
  model: "m",
  apiKey: "k",
};

describe("validateForm", () => {
  it("flags empty name", () => {
    expect(validateForm({ ...ok, name: "" }, false)).toEqual({
      name: "required",
    });
  });
  it("flags url without scheme", () => {
    expect(validateForm({ ...ok, baseUrl: "api.openai.com" }, false)).toEqual({
      baseUrl: "url",
    });
  });
  it("allows empty key for localhost", () => {
    expect(
      validateForm(
        { ...ok, baseUrl: "http://127.0.0.1:11434/v1", apiKey: "" },
        false,
      ),
    ).toEqual({});
  });
  it("allows empty key when one is already stored", () => {
    expect(validateForm({ ...ok, apiKey: "" }, true)).toEqual({});
  });
  it("requires key for remote when none stored", () => {
    expect(validateForm({ ...ok, apiKey: "" }, false)).toEqual({
      apiKey: "required",
    });
  });
  it("rejects plain http for remote hosts", () => {
    expect(
      validateForm({ ...ok, baseUrl: "http://example.com/v1" }, false),
    ).toEqual({ baseUrl: "url" });
  });
});
