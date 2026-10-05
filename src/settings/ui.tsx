import { useCallback, useEffect, useRef, useState } from "react";
import { S } from "../shared/strings";

/** "Сохранено" label that shows for 1.5 s after `flash()`. */
export function useSavedFlash(): [boolean, () => void] {
  const [on, setOn] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const flash = useCallback(() => {
    setOn(true);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setOn(false), 1500);
  }, []);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  return [on, flash];
}

export function SavedLabel({ on }: { on: boolean }) {
  return (
    <span className={`saved-flash${on ? " on" : ""}`} aria-live="polite">
      {on ? S.saved : ""}
    </span>
  );
}

export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: [T, string][];
  onChange: (v: T) => void;
}) {
  return (
    <fieldset className="segmented">
      <legend className="seg-legend">{label}</legend>
      {options.map(([v, text]) => (
        <label key={v} className={value === v ? "seg on" : "seg"}>
          <input
            type="radio"
            className="seg-input"
            name={label}
            checked={value === v}
            onChange={() => onChange(v)}
          />
          {text}
        </label>
      ))}
    </fieldset>
  );
}
