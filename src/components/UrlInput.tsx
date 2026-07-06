import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { parseCurlCommand, varsEffective } from "../ipc/commands";
import type { RequestSpec, Variable } from "../types";
import {
  splitUrl,
  useAliasForHost,
  useHostAliases,
  ALIAS_COLORS,
} from "../state/hostAliases";
import { UrlDisplay } from "./UrlDisplay";

interface Props {
  value: string;
  collectionId: number | null;
  onChange: (url: string) => void;
  /** A curl command was pasted — the whole request should be replaced. */
  onCurl: (spec: RequestSpec) => void;
}

/**
 * URL input with `{{variable}}` autocomplete, curl paste, and host aliases:
 * the host under the caret is highlighted and an alias can be saved; once
 * aliased, the blurred bar collapses the host into a coloured chip.
 */
export function UrlInput({ value, collectionId, onChange, onCurl }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const underlayRef = useRef<HTMLDivElement>(null);
  const hostRef = useRef<HTMLSpanElement>(null);
  const dismissedHost = useRef<string | null>(null);

  const [vars, setVars] = useState<Variable[]>([]);
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const [tokenStart, setTokenStart] = useState(0);
  const [highlight, setHighlight] = useState(0);

  const [focused, setFocused] = useState(false);
  const [caretInHost, setCaretInHost] = useState(false);
  const [aliasOpen, setAliasOpen] = useState(false);
  const [popLeft, setPopLeft] = useState(0);

  const { pre, host, post } = splitUrl(value);
  const alias = useAliasForHost(host);
  const upsert = useHostAliases((s) => s.upsert);
  const remove = useHostAliases((s) => s.remove);

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

  // Keep the underlay aligned with the input's horizontal scroll.
  const syncScroll = () => {
    if (underlayRef.current && inputRef.current)
      underlayRef.current.scrollLeft = inputRef.current.scrollLeft;
  };
  useLayoutEffect(syncScroll, [value, caretInHost]);

  const openPopover = () => {
    dismissedHost.current = null;
    setAliasOpen(true);
    requestAnimationFrame(() => {
      const el = hostRef.current;
      const wrap = el?.offsetParent as HTMLElement | null;
      if (el && wrap) {
        const left = el.offsetLeft - (inputRef.current?.scrollLeft ?? 0);
        setPopLeft(Math.max(6, Math.min(left, wrap.clientWidth - 256)));
      }
    });
  };

  const dismissPopover = () => {
    setAliasOpen(false);
    dismissedHost.current = host;
  };

  // Recompute caret-in-host state (and open/close the alias popover).
  const updateCaret = () => {
    const caret = inputRef.current?.selectionStart ?? 0;
    const inHost =
      host != null && caret >= pre.length && caret <= pre.length + host.length;
    setCaretInHost(inHost);
    if (inHost) {
      if (!aliasOpen && dismissedHost.current !== host) openPopover();
    } else {
      if (aliasOpen) setAliasOpen(false);
      dismissedHost.current = null;
    }
  };

  // Close the popover on an outside click.
  useEffect(() => {
    if (!aliasOpen) return;
    const onDown = (e: MouseEvent) => {
      const wrap = inputRef.current?.closest(".url-input-wrap");
      if (wrap && !wrap.contains(e.target as Node)) setAliasOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [aliasOpen]);

  const collapsed = !focused && !aliasOpen && !!host && !!alias;

  return (
    <div className="url-input-wrap">
      <input
        ref={inputRef}
        className="url-input"
        value={value}
        placeholder="https://api.example.com/v1/users?limit=10 — or paste a curl command"
        spellCheck={false}
        style={collapsed ? { opacity: 0 } : undefined}
        onChange={(e) => {
          onChange(e.target.value);
          detectToken(e.target.value, e.target.selectionStart ?? 0);
          requestAnimationFrame(updateCaret);
        }}
        onSelect={updateCaret}
        onClick={updateCaret}
        onScroll={syncScroll}
        onFocus={() => {
          setFocused(true);
          requestAnimationFrame(updateCaret);
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape" && aliasOpen) {
            dismissPopover();
            return;
          }
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
        onBlur={() =>
          setTimeout(() => {
            setOpen(false);
            setFocused(false);
          }, 150)
        }
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

      {/* Transparent underlay tinting the host under the caret. */}
      <div className="url-underlay" ref={underlayRef} aria-hidden="true">
        {pre}
        {host != null && (
          <span ref={hostRef} className={caretInHost ? "uh-host" : undefined}>
            {host}
          </span>
        )}
        {post}
      </div>

      {collapsed && (
        <button
          type="button"
          className="url-collapsed"
          title="Click to edit"
          onMouseDown={(e) => {
            e.preventDefault();
            setFocused(true);
            inputRef.current?.focus();
          }}
        >
          <UrlDisplay url={value} scheme="dim" />
        </button>
      )}

      {aliasOpen && host && (
        <AliasPopover
          host={host}
          left={popLeft}
          initialName={alias?.alias ?? ""}
          initialColor={alias?.color || ALIAS_COLORS[0]}
          existingId={alias?.id ?? null}
          onSave={(name, color) => {
            void upsert(host, name, color);
            dismissPopover();
          }}
          onRemove={() => {
            if (alias) void remove(alias.id);
            dismissPopover();
          }}
          onClose={dismissPopover}
        />
      )}

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

function AliasPopover({
  host,
  left,
  initialName,
  initialColor,
  existingId,
  onSave,
  onRemove,
  onClose,
}: {
  host: string;
  left: number;
  initialName: string;
  initialColor: string;
  existingId: number | null;
  onSave: (name: string, color: string) => void;
  onRemove: () => void;
  onClose: () => void;
}) {
  const [name, setName] = useState(initialName);
  const [color, setColor] = useState(initialColor);

  return (
    <div className="alias-popover" style={{ left }}>
      <div className="alias-popover-host">{host}</div>
      <input
        className="alias-name"
        value={name}
        placeholder="alias (e.g. prod)"
        spellCheck={false}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && name.trim()) onSave(name.trim(), color);
          else if (e.key === "Escape") onClose();
        }}
      />
      <div className="alias-swatches">
        {ALIAS_COLORS.map((c) => (
          <button
            key={c}
            type="button"
            className={`alias-swatch${c === color ? " active" : ""}`}
            style={{ background: c }}
            onClick={() => setColor(c)}
          />
        ))}
      </div>
      <div className="alias-popover-actions">
        {existingId != null && (
          <button type="button" className="danger" onClick={onRemove}>
            Remove
          </button>
        )}
        <span className="spacer" />
        <button type="button" onClick={onClose}>
          Cancel
        </button>
        <button
          type="button"
          className="primary"
          disabled={!name.trim()}
          onClick={() => onSave(name.trim(), color)}
        >
          Save
        </button>
      </div>
    </div>
  );
}

function effectiveValue(v: Variable): string {
  const val = v.current_value ?? v.initial_value;
  return val.length > 40 ? `${val.slice(0, 40)}…` : val;
}
