// Mirrors Rust validate_provider for inline errors. Rust stays the source of truth.
export interface FormValues {
  name: string;
  baseUrl: string;
  model: string;
  apiKey: string;
}

export type ErrorCode = "required" | "url" | "too_long" | "store";
export type FormErrors = Partial<
  Record<"name" | "baseUrl" | "model" | "apiKey", ErrorCode>
>;

const MAX_KEY_LEN = 1000;
const LOCAL_HOSTS = new Set(["localhost", "127.0.0.1", "::1"]);

export function normalizeBaseUrl(url: string): string {
  return url.trim().replace(/\/+$/, "");
}

function parseUrl(url: string): { https: boolean; host: string } | null {
  if (!/^https?:\/\//i.test(url)) return null;
  try {
    const u = new URL(url);
    const host = u.hostname.replace(/^\[|\]$/g, "").toLowerCase();
    if (!host) return null;
    return { https: u.protocol === "https:", host };
  } catch {
    return null;
  }
}

export function validateForm(v: FormValues, hasKey: boolean): FormErrors {
  const errs: FormErrors = {};
  if (!v.name.trim()) errs.name = "required";
  const url = normalizeBaseUrl(v.baseUrl);
  let local = false;
  if (!url) {
    errs.baseUrl = "required";
  } else {
    const p = parseUrl(url);
    if (!p) {
      errs.baseUrl = "url";
    } else {
      local = LOCAL_HOSTS.has(p.host);
      if (!p.https && !local) errs.baseUrl = "url";
    }
  }
  if (!v.model.trim()) errs.model = "required";
  const key = v.apiKey.trim();
  if (key) {
    if ([...key].length > MAX_KEY_LEN) errs.apiKey = "too_long";
  } else if (!hasKey && !local) {
    errs.apiKey = "required";
  }
  return errs;
}
