import { describe, expect, it } from "vitest";
import { S } from "../shared/strings";
import { splitMessage, TOAST_KINDS, toastContent } from "./toast";

describe("toastContent", () => {
  it("network: retry button, 12 s", () => {
    const c = toastContent("network");
    expect(c.reason).toBe("Нет соединения с сервером.");
    expect(c.button).toBe(S.retry);
    expect(c.buttonAction).toBe("retry");
    expect(c.timeoutMs).toBe(12000);
  });

  it("auth: settings button, 12 s", () => {
    const c = toastContent("auth");
    expect(c.button).toBe(S.openSettings);
    expect(c.buttonAction).toBe("settings");
    expect(c.timeoutMs).toBe(12000);
  });

  it("capture_password: no button, 8 s", () => {
    const c = toastContent("capture_password");
    expect(c.button).toBeNull();
    expect(c.buttonAction).toBeNull();
    expect(c.timeoutMs).toBe(8000);
  });

  it("focus_lost_copied: paste-there button, 12 s", () => {
    const c = toastContent("focus_lost_copied");
    expect(c.button).toBe(S.pasteThere);
    expect(c.buttonAction).toBe("paste_there");
    expect(c.timeoutMs).toBe(12000);
  });

  it("explicit action from Rust wins", () => {
    expect(toastContent("network", "none").button).toBeNull();
  });

  it("every ToastKind has a non-empty reason", () => {
    expect(TOAST_KINDS).toHaveLength(19);
    for (const k of TOAST_KINDS) {
      expect(toastContent(k).reason.length).toBeGreaterThan(0);
    }
  });

  it("splitMessage keeps a single sentence as reason", () => {
    expect(splitMessage("Один.")).toEqual({ reason: "Один.", hint: "" });
  });
});
