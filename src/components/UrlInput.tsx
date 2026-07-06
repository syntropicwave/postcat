import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { parseCurlCommand, varsEffective } from "../ipc/commands";
import type { RequestSpec, Variable } from "../types";
import {
  matchAlias,
  originRange,
  buildUrlSuggestions,
  useAliasByKey,
  useHostAliases,
  ALIAS_COLORS,
  type UrlSuggestion,
} from "../state/hostAliases";
import { HostChip } from "./HostChip";
import { UrlDisplay } from "./UrlDisplay";

interface Props {
  value: string;
  collectionId: number | null;
  onChange: (url: string) => void;
  /** A curl command was pasted — the whole request should be replaced. */
  onCurl: (spec: RequestSpec) => void;
}

interface Range {
  start: number;
  end: number;
}

type Target = Range & { kind: "selection" | "alias" | "origin" };

/**
 * Address bar with three assists:
 *  - Intellisense: while typing a new address, suggest saved alias prefixes
 *    and schemes, filtered as you type.
 *  - Aliases: select any span (or click the host) to save it as a coloured
 *    prefix; the blurred bar then collapses it into a chip.
 *  - `{{variable}}` autocomplete and curl paste.
 */
export function UrlInput({ value, collectionId, onChange, onCurl }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const underlayRef = useRef<HTMLDivElement>(null);
  const targetRef = useRef<HTMLSpanElement>(null);
  const dismissedKey = useRef<string | null>(null);

  const [vars, setVars] = useState<Variable[]>([]);
  const [varToken, setVarToken] = useState<{ start: number } | null>(null);
  const [varFilter, setVarFilter] = useState("");
  const [varHi, setVarHi] = useState(0);

  const [focused, setFocused] = useState(false);
  const [sel, setSel] = useState<Range>({ start: 0, end: 0 });
  const [urlHi, setUrlHi] = useState(0);
  const [urlDismissed, setUrlDismissed] = useState(false);

  const [aliasOpen, setAliasOpen] = useState(false);
  const [aliasTarget, setAliasTarget] = useState<Target | null>(null);
  const [popLeft, setPopLeft] = useState(0);

  const aliases = useHostAliases((s) => s.aliases);
  const upsert = useHostAliases((s) => s.upsert);
  const remove = useHostAliases((s) => s.remove);

  // ---- derived state ----
  const match = matchAlias(value, aliases);
  const varMatches = vars.filter((v) =>
    v.key.toLowerCase().startsWith(varFilter.toLowerCase()),
  );
  const caretAtEnd = sel.start === sel.end && sel.start === value.length;
  const urlSuggestions = focused ? buildUrlSuggestions(value, aliases) : [];
  const showVar = !!varToken && varMatches.length > 0;
  const showUrl =
    focused &&
    !varToken &&
    !urlDismissed &&
    caretAtEnd &&
    urlSuggestions.length > 0;

  const liveTarget = ((): Target | null => {
    if (sel.end > sel.start)
      return { start: sel.start, end: sel.end, kind: "selection" };
    const caret = sel.start;
    if (match && caret >= match.start && caret <= match.end)
      return { start: match.start, end: match.end, kind: "alias" };
    const o = originRange(value);
    if (o && caret >= o.start && caret <= o.end)
      return { ...o, kind: "origin" };
    return null;
  })();
  // Don't pop the "create alias" prompt for the origin while typing at the end
  // — that's when intellisense belongs; only when the caret lands back in it.
  const wantOpen =
    !!liveTarget && !(liveTarget.kind === "origin" && caretAtEnd);
  const hlTarget = aliasOpen ? aliasTarget : null;
  const hlIsSelection = aliasTarget?.kind === "selection";

  const subject =
    aliasOpen && aliasTarget
      ? value.slice(aliasTarget.start, aliasTarget.end)
      : "";
  const existing = useAliasByKey(subject);

  const collapsed = !focused && !aliasOpen && !!match;

  // Refresh variables when the {{ dropdown opens.
  useEffect(() => {
    if (varToken) varsEffective(collectionId).then(setVars);
  }, [varToken, collectionId]);

  // Open/close the alias popover as the caret/selection moves. This mirrors
  // the caret into open/target state that must survive the input losing focus
  // (e.g. clicking into the popover), so the setState-in-effect is deliberate.
  /* eslint-disable react-hooks/set-state-in-effect */
  useEffect(() => {
    if (!focused) return;
    if (varToken || showUrl || !wantOpen || !liveTarget) {
      if (!aliasOpen) dismissedKey.current = null;
      setAliasOpen(false);
      return;
    }
    const key = value.slice(liveTarget.start, liveTarget.end).toLowerCase();
    if (dismissedKey.current === key) return;
    setAliasTarget(liveTarget);
    setAliasOpen(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focused, varToken, showUrl, wantOpen, sel.start, sel.end, value]);
  /* eslint-enable react-hooks/set-state-in-effect */

  const syncScroll = () => {
    if (underlayRef.current && inputRef.current)
      underlayRef.current.scrollLeft = inputRef.current.scrollLeft;
  };
  useLayoutEffect(syncScroll, [value, hlTarget]);

  // Anchor the popover under the highlighted target.
  useLayoutEffect(() => {
    if (!aliasOpen) return;
    const el = targetRef.current;
    const wrap = el?.offsetParent as HTMLElement | null;
    if (el && wrap) {
      const left = el.offsetLeft - (inputRef.current?.scrollLeft ?? 0);
      setPopLeft(Math.max(6, Math.min(left, wrap.clientWidth - 262)));
    }
  }, [aliasOpen, aliasTarget, value]);

  const updateSel = () => {
    const el = inputRef.current;
    if (el)
      setSel({ start: el.selectionStart ?? 0, end: el.selectionEnd ?? 0 });
  };

  const detectVarToken = (text: string, caret: number) => {
    const m = text.slice(0, caret).match(/\{\{([A-Za-z0-9_.-]*)$/);
    if (m) {
      setVarToken({ start: caret - m[1].length });
      setVarFilter(m[1]);
      setVarHi(0);
    } else {
      setVarToken(null);
    }
  };

  const insertVar = (key: string) => {
    if (!varToken) return;
    const caret = inputRef.current?.selectionStart ?? value.length;
    const after = value.slice(caret);
    const needsClose = !after.startsWith("}}");
    const next =
      value.slice(0, varToken.start) + key + (needsClose ? "}}" : "") + after;
    onChange(next);
    setVarToken(null);
    requestAnimationFrame(() => {
      const pos = varToken.start + key.length + (needsClose ? 2 : 0);
      inputRef.current?.setSelectionRange(pos, pos);
      inputRef.current?.focus();
    });
  };

  const applyUrlSuggestion = (s: UrlSuggestion) => {
    onChange(s.key);
    setUrlDismissed(false);
    requestAnimationFrame(() => {
      inputRef.current?.setSelectionRange(s.key.length, s.key.length);
      inputRef.current?.focus();
      updateSel();
    });
  };

  const dismissAlias = () => {
    setAliasOpen(false);
    if (subject) dismissedKey.current = subject.toLowerCase();
  };

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
          setFocused(true);
          onChange(e.target.value);
          setUrlDismissed(false);
          detectVarToken(e.target.value, e.target.selectionStart ?? 0);
          setSel({
            start: e.target.selectionStart ?? 0,
            end: e.target.selectionEnd ?? 0,
          });
        }}
        onSelect={updateSel}
        onClick={() => {
          // Cover the case where the input is already DOM-focused (React reuses
          // it across tab switches), so onFocus never re-fires.
          setFocused(true);
          updateSel();
        }}
        onScroll={syncScroll}
        onFocus={() => {
          setFocused(true);
          updateSel();
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            if (showVar) return setVarToken(null);
            if (showUrl) return setUrlDismissed(true);
            if (aliasOpen) return dismissAlias();
            return;
          }
          if (showVar) {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setVarHi((h) => (h + 1) % varMatches.length);
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setVarHi((h) => (h - 1 + varMatches.length) % varMatches.length);
            } else if (e.key === "Enter" || e.key === "Tab") {
              e.preventDefault();
              insertVar(varMatches[varHi].key);
            }
            return;
          }
          if (showUrl) {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setUrlHi((h) => (h + 1) % urlSuggestions.length);
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setUrlHi(
                (h) => (h - 1 + urlSuggestions.length) % urlSuggestions.length,
              );
            } else if (e.key === "Enter" || e.key === "Tab") {
              e.preventDefault();
              applyUrlSuggestion(
                urlSuggestions[Math.min(urlHi, urlSuggestions.length - 1)],
              );
            }
          }
        }}
        onBlur={() => {
          // Dropdown/popover items use onMouseDown+preventDefault, so clicking
          // them doesn't blur the input — no debounce needed here.
          setVarToken(null);
          setFocused(false);
        }}
        onPaste={(e) => {
          const text = e.clipboardData.getData("text");
          if (text.trimStart().startsWith("curl")) {
            e.preventDefault();
            parseCurlCommand(text)
              .then(onCurl)
              .catch(() => onChange(text));
          }
        }}
      />

      {/* Transparent underlay that tints the aliasing target. */}
      <div className="url-underlay" ref={underlayRef} aria-hidden="true">
        {hlTarget ? (
          <>
            {value.slice(0, hlTarget.start)}
            <span
              ref={targetRef}
              className={hlIsSelection ? undefined : "uh-host"}
            >
              {value.slice(hlTarget.start, hlTarget.end)}
            </span>
            {value.slice(hlTarget.end)}
          </>
        ) : (
          value
        )}
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

      {aliasOpen && aliasTarget && subject && (
        <AliasPopover
          key={subject}
          subject={subject}
          left={popLeft}
          initialName={existing?.alias ?? ""}
          initialColor={existing?.color || ALIAS_COLORS[0]}
          existingId={existing?.id ?? null}
          onSave={(name, color) => {
            void upsert(subject, name, color);
            dismissAlias();
          }}
          onRemove={() => {
            if (existing) void remove(existing.id);
            dismissAlias();
          }}
          onClose={dismissAlias}
        />
      )}

      {showUrl && (
        <div className="var-suggest url-suggest">
          {urlSuggestions.slice(0, 8).map((s, i) => (
            <div
              key={`${s.kind}:${s.key}`}
              className={`var-suggest-item${i === urlHi ? " active" : ""}`}
              onMouseDown={(e) => {
                e.preventDefault();
                applyUrlSuggestion(s);
              }}
            >
              {s.kind === "alias" && s.alias ? (
                <>
                  <span className="url-suggest-chip">
                    <HostChip
                      alias={s.alias.alias}
                      color={s.alias.color}
                      host={s.alias.host}
                    />
                  </span>
                  <span className="var-value">{s.key}</span>
                </>
              ) : (
                <>
                  <span className="var-key">{s.key}</span>
                  <span className="var-value">scheme</span>
                </>
              )}
            </div>
          ))}
        </div>
      )}

      {showVar && (
        <div className="var-suggest">
          {varMatches.slice(0, 8).map((v, i) => (
            <div
              key={v.key}
              className={`var-suggest-item${i === varHi ? " active" : ""}`}
              onMouseDown={(e) => {
                e.preventDefault();
                insertVar(v.key);
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
  subject,
  left,
  initialName,
  initialColor,
  existingId,
  onSave,
  onRemove,
  onClose,
}: {
  subject: string;
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

  // Close on an outside click.
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      const el = e.target as HTMLElement;
      if (!el.closest(".alias-popover") && !el.closest(".url-input")) onClose();
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [onClose]);

  return (
    <div className="alias-popover" style={{ left }}>
      <div className="alias-popover-host">{subject}</div>
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
