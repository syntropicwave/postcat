import { useEffect, useState } from "react";
import { appInfo, type AppInfo } from "./ipc/commands";
import "./App.css";

function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    appInfo()
      .then(setInfo)
      .catch((e) => setError(String(e)));
  }, []);

  return (
    <main className="container">
      <h1>postcat</h1>
      <p>Local-first API client. History that remembers everything.</p>
      {info && (
        <p className="app-info">
          v{info.version} · schema v{info.schema_version} · {info.db_path}
        </p>
      )}
      {error && <p className="app-error">Core error: {error}</p>}
    </main>
  );
}

export default App;
