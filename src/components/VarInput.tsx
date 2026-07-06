import { useEffect, useRef, useState } from "react";
import { varsEffective } from "../ipc/commands";
import type { Variable } from "../types";

interface Props {
  value: string;
  onChange: (value: string) => void;
  collectionId?: number | null;
  placeholder?: string;
  className?: string;
  type?: "text" | "password";
  spellCheck?: boolean;
  onKeyDown?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
}

/**
 * Text input with `{{variable}}` autocomplete. Used everywhere variables are
 * accepted (header/param values, auth fields, URL) so completion is uniform.
 */
export function VarInput({
  value,
  onChange,
  collectionId = null,
  placeholder,
  className,
  type = "text",
  spellCheck,
  onKeyDown,
}: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [vars, setVars] = useState<Variable[]>([]);
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const [tokenStart, setTokenStart] = useState(0);
  const [highlight, setHighlight] = useState(0);

  useEffect(() => {
    if (open) varsEffective(collectionId).then(setVars);
  }, [open, collectionId]);

  const matches = vars.filter((v) =>
    v.key.toLowerCase().startsWith(filter.toLowerCase()),
  );

  const detectToken = (text: string, caret: number) => {
    const before = text.slice(0, caret);
    const m = before.match(/\{\{([A-Za-z0-9_.$-]*)$/);
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
    <div className="var-input-wrap">
      <input
        ref={inputRef}
        type={type}
        className={className}
        value={value}
        placeholder={placeholder}
        spellCheck={spellCheck}
        onChange={(e) => {
          onChange(e.target.value);
          detectToken(e.target.value, e.target.selectionStart ?? 0);
        }}
        onKeyDown={(e) => {
          if (open && matches.length > 0) {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setHighlight((h) => (h + 1) % matches.length);
              return;
            }
            if (e.key === "ArrowUp") {
              e.preventDefault();
              setHighlight((h) => (h - 1 + matches.length) % matches.length);
              return;
            }
            if (e.key === "Enter" || e.key === "Tab") {
              e.preventDefault();
              insert(matches[highlight].key);
              return;
            }
            if (e.key === "Escape") {
              setOpen(false);
              return;
            }
          }
          onKeyDown?.(e);
        }}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
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
                {v.is_secret
                  ? "••••••"
                  : truncate(v.current_value ?? v.initial_value)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function truncate(s: string): string {
  return s.length > 40 ? `${s.slice(0, 40)}…` : s;
}
