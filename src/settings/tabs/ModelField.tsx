import { useEffect, useRef, useState } from "react";
import {
  type PromptItem,
  type ProvidersView,
  promptSetModel,
} from "../../shared/ipc";
import { S } from "../../shared/strings";
import { debounce } from "../logic";

/** PROV-03: «Как по умолчанию» or provider + model (from the cached list or typed). */
export function ModelField({
  item,
  providers,
  onChanged,
}: {
  item: PromptItem;
  providers: ProvidersView;
  onChanged: () => void;
}) {
  const [providerId, setProviderId] = useState(item.provider_id ?? "");
  const [model, setModel] = useState(item.model ?? "");
  const save = useRef(
    debounce((pid: string, m: string) => {
      void promptSetModel(item.id, pid || null, m.trim() || null).then(
        onChanged,
      );
    }, 400),
  ).current;
  useEffect(() => () => save.flush(), [save]);

  const def =
    providers.items.find((p) => p.id === providers.default_id) ??
    providers.items[0];
  const chosen = providers.items.find((p) => p.id === providerId);
  const missing = item.provider_missing && !!providerId && !chosen;
  const listId = `cmd-models-${item.id}`;

  return (
    <div className="field">
      <span className="field-label">{S.editorModel}</span>
      <div className="model-row">
        <select
          className="field-input"
          aria-label={S.editorProvider}
          value={chosen ? providerId : ""}
          onChange={(e) => {
            const pid = e.target.value;
            setProviderId(pid);
            setModel("");
            save.cancel();
            void promptSetModel(item.id, pid || null, null).then(onChanged);
          }}
        >
          <option value="">
            {def
              ? S.modelDefaultOf
                  .replace("{name}", def.name)
                  .replace("{model}", def.model)
              : S.modelDefault}
          </option>
          {providers.items.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        {chosen && (
          <>
            <input
              className="field-input"
              aria-label={S.editorModel}
              value={model}
              list={listId}
              spellCheck={false}
              placeholder={S.modelPlaceholder.replace("{model}", chosen.model)}
              onChange={(e) => {
                setModel(e.target.value);
                save(providerId, e.target.value);
              }}
              onBlur={() => save.flush()}
            />
            <datalist id={listId}>
              {chosen.models.map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
          </>
        )}
      </div>
      {missing && <span className="field-error">{S.providerRemoved}</span>}
    </div>
  );
}
