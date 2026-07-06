import { useEffect, useRef, useState } from "react";
import { parseCurlCommand, varsEffective } from "../ipc/commands";
import type { RequestSpec, Variable } from "../types";

interface Props {
  value: string;
  collectionId: number | null;
  onChange: (url: string) => void;
  /** A curl command was pasted — the whole request should be replaced. */
  onCurl: (spec: RequestSpec) => void;
}

/** URL input with `{{variable}}` autocomplete and paste-a-curl support. */
export function UrlInput({ value, collectionId, onChange, onCurl }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [vars, setVars] = useState<Variable[]>([]);
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const [tokenStart, setTokenStart] = useState(0);
  const [highlight, setHighlight] = useState(0);

  // Refresh the variable list whenever the dropdown opens.
  useEffect(() => {
    if (open) varsEffective(collectionId).then(setVars);
  }, [open, collectionId]);

  const matches = vars.filter((v) =>
    v.key.toLowerCase().startsWith(filter.toLowerCase()),
  );

  const detectToken = (text: string, caret: number) => {
    const before = text.slice(0, caret);
    const m = before.match(/\{\{([A-Za-z0-9_.-]*)$/);
    if (m) {
      setTokenStart(caret - m[1].length);
      setFilter(m[1]);
      setHighlight(0);
      setOpen(true);
    } else {
      setOpen(false);
    }
  };

  const insert = (key: string) => {
    const caret = inputRef.current?.selectionStart ?? value.length;
    const after = value.slice(caret);
    const needsClose = !after.startsWith("}}");
    const next =
      value.slice(0, tokenStart) + key + (needsClose ? "}}" : "") + after;
    onChange(next);
    setOpen(false);
    requestAnimationFrame(() => {
      const pos = tokenStart + key.length + (needsClose ? 2 : 0);
      inputRef.current?.setSelectionRange(pos, pos);
      inputRef.current?.focus();
    });
  };

  return (
    <div className="url-input-wrap">
      <input
        ref={inputRef}
        className="url-input"
        value={value}
        placeholder="https://api.example.com/v1/users?limit=10 — or paste a curl command"
        spellCheck={false}
        onChange={(e) => {
          onChange(e.target.value);
          detectToken(e.target.value, e.target.selectionStart ?? 0);
        }}
        onKeyDown={(e) => {
          if (!open || matches.length === 0) return;
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setHighlight((h) => (h + 1) % matches.length);
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setHighlight((h) => (h - 1 + matches.length) % matches.length);
          } else if (e.key === "Enter" || e.key === "Tab") {
            e.preventDefault();
            insert(matches[highlight].key);
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
        onPaste={(e) => {
          const text = e.clipboardData.getData("text");
          if (text.trimStart().startsWith("curl")) {
            e.preventDefault();
            parseCurlCommand(text)
              .then(onCurl)
              .catch(() => onChange(text)); // not parseable — paste as-is
          }
        }}
      />
      {open && matches.length > 0 && (
        <div className="var-suggest">
          {matches.slice(0, 8).map((v, i) => (
            <div
              key={v.key}
              className={`var-suggest-item${i === highlight ? " active" : ""}`}
              onMouseDown={(e) => {
                e.preventDefault();
                insert(v.key);
              }}
            >
              <span className="var-key">{v.key}</span>
              <span className="var-value">
                {v.is_secret ? "••••••" : effectiveValue(v)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function effectiveValue(v: Variable): string {
  const val = v.current_value ?? v.initial_value;
  return val.length > 40 ? `${val.slice(0, 40)}…` : val;
}
