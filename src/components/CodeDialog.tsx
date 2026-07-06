import { useMemo, useState } from "react";
import { specFromTab, type Tab } from "../state/tabs";
import {
  CODEGEN_LANGUAGES,
  generateCode,
  type CodegenLanguage,
} from "../utils/codegen";

export function CodeDialog({
  tab,
  onClose,
}: {
  tab: Tab;
  onClose: () => void;
}) {
  const [lang, setLang] = useState<CodegenLanguage>("cURL");
  const [copied, setCopied] = useState(false);

  const code = useMemo(() => generateCode(lang, specFromTab(tab)), [lang, tab]);

  const copy = () => {
    const done = () => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    };
    if (navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(code).then(done, () => {
        fallbackCopy(code);
        done();
      });
    } else {
      fallbackCopy(code);
      done();
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <div className="code-toolbar">
          <select
            value={lang}
            onChange={(e) => setLang(e.target.value as CodegenLanguage)}
          >
            {CODEGEN_LANGUAGES.map((l) => (
              <option key={l}>{l}</option>
            ))}
          </select>
          <span style={{ flex: 1 }} />
          <button className="primary" onClick={copy}>
            {copied ? "Copied!" : "Copy"}
          </button>
          <button onClick={onClose}>Close</button>
        </div>
        <pre className="code-preview">{code}</pre>
      </div>
    </div>
  );
}

function fallbackCopy(text: string) {
  const ta = document.createElement("textarea");
  ta.value = text;
  document.body.appendChild(ta);
  ta.select();
  document.execCommand("copy");
  ta.remove();
}
