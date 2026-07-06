import { useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { usePrefersDark } from "../hooks/usePrefersDark";

interface Props {
  preRequestScript: string;
  testScript: string;
  onChange: (pre: string, test: string) => void;
}

const PLACEHOLDER_PRE = `// Runs before the request is sent. Examples:
// pm.request.headers.upsert({key: "X-Trace", value: pm.variables.replaceIn("{{$guid}}")});
// pm.environment.set("ts", Date.now());`;

const PLACEHOLDER_TEST = `// Runs after the response arrives. Examples:
// pm.test("status is 200", () => pm.response.to.have.status(200));
// pm.test("has user id", () => pm.expect(pm.response.json().id).to.be.a("number"));
// pm.collectionVariables.set("token", pm.response.json().token);`;

export function ScriptsEditor({
  preRequestScript,
  testScript,
  onChange,
}: Props) {
  const [tab, setTab] = useState<"pre" | "test">("test");
  const dark = usePrefersDark();

  return (
    <div className="scripts-editor">
      <div className="scripts-tabs">
        <button
          className={tab === "pre" ? "active" : ""}
          onClick={() => setTab("pre")}
        >
          Pre-request{preRequestScript ? " •" : ""}
        </button>
        <button
          className={tab === "test" ? "active" : ""}
          onClick={() => setTab("test")}
        >
          Tests{testScript ? " •" : ""}
        </button>
        <span className="auth-hint">
          pm.test / pm.expect / pm.request / pm.response / pm.*Variables /
          pm.sendRequest
        </span>
      </div>
      {tab === "pre" ? (
        <CodeMirror
          key="pre"
          className="body-code"
          value={preRequestScript}
          height="100%"
          theme={dark ? "dark" : "light"}
          placeholder={PLACEHOLDER_PRE}
          extensions={[javascript()]}
          onChange={(v) => onChange(v, testScript)}
        />
      ) : (
        <CodeMirror
          key="test"
          className="body-code"
          value={testScript}
          height="100%"
          theme={dark ? "dark" : "light"}
          placeholder={PLACEHOLDER_TEST}
          extensions={[javascript()]}
          onChange={(v) => onChange(preRequestScript, v)}
        />
      )}
    </div>
  );
}
