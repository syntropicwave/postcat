import { useEffect, useState } from "react";
import {
  syncLogin,
  syncLogout,
  syncNow,
  syncRegister,
  syncStatus,
  type SyncStatus,
} from "../ipc/commands";
import { useTabs } from "../state/tabs";

type Mode = "login" | "register";

export function SyncDialog({ onClose }: { onClose: () => void }) {
  const bumpCollections = useTabs((s) => s.bumpCollections);
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [mode, setMode] = useState<Mode>("login");
  const [url, setUrl] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [recovery, setRecovery] = useState<string | null>(null);
  const [report, setReport] = useState<string | null>(null);

  const refresh = () =>
    syncStatus().then((s) => {
      setStatus(s);
      if (s.url) setUrl(s.url);
      if (s.email) setEmail(s.email);
    });

  useEffect(() => {
    refresh();
  }, []);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      if (mode === "register") {
        const code = await syncRegister(url.trim(), email.trim(), password);
        setRecovery(code);
      } else {
        await syncLogin(url.trim(), email.trim(), password);
      }
      setPassword("");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doSync = async () => {
    setBusy(true);
    setError(null);
    setReport(null);
    try {
      const r = await syncNow();
      setReport(`Pushed ${r.pushed}, pulled ${r.pulled}.`);
      bumpCollections();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const signedIn = status?.signed_in ?? false;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="retention-title">Sync</div>
        <p className="import-hint">
          Optional, end-to-end encrypted. Your password is the key and never
          leaves this device — the server stores only ciphertext. Point the URL
          at your own <code>postcat-sync</code> server or a hosted one.
        </p>

        {recovery ? (
          <div className="sync-recovery">
            <div className="retention-title">Save your recovery code</div>
            <p className="import-hint">
              This is the only way back in if you forget your password. It is
              shown once and cannot be recovered.
            </p>
            <pre className="recovery-code">{recovery}</pre>
            <div className="retention-actions">
              <button
                className="primary"
                onClick={() => {
                  navigator.clipboard?.writeText(recovery);
                }}
              >
                Copy
              </button>
              <button onClick={() => setRecovery(null)}>I saved it</button>
            </div>
          </div>
        ) : signedIn ? (
          <div className="sync-panel">
            <div className="sync-row">
              <span className="sync-label">Signed in</span>
              <span className="sync-value">{status?.email}</span>
            </div>
            <div className="sync-row">
              <span className="sync-label">Server</span>
              <span className="sync-value">{status?.url}</span>
            </div>
            <div className="sync-row">
              <span className="sync-label">Pending changes</span>
              <span className="sync-value">{status?.pending ?? 0}</span>
            </div>
            {report && <div className="collections-status">{report}</div>}
            {error && <div className="app-error">{error}</div>}
            <div className="retention-actions">
              <button
                onClick={async () => {
                  await syncLogout();
                  setReport(null);
                  await refresh();
                }}
              >
                Sign out
              </button>
              <span style={{ flex: 1 }} />
              <button
                className="primary"
                disabled={busy}
                onClick={() => void doSync()}
              >
                {busy ? "Syncing…" : "Sync now"}
              </button>
            </div>
          </div>
        ) : (
          <div className="sync-panel">
            <div className="sidebar-tabs">
              <button
                className={mode === "login" ? "active" : ""}
                onClick={() => setMode("login")}
              >
                Sign in
              </button>
              <button
                className={mode === "register" ? "active" : ""}
                onClick={() => setMode("register")}
              >
                Create account
              </button>
            </div>
            <label className="modal-field">
              Server URL
              <input
                value={url}
                placeholder="https://sync.example.com:8787"
                onChange={(e) => setUrl(e.target.value)}
              />
            </label>
            <label className="modal-field">
              Email
              <input
                value={email}
                autoComplete="username"
                onChange={(e) => setEmail(e.target.value)}
              />
            </label>
            <label className="modal-field">
              Password
              <input
                type="password"
                value={password}
                autoComplete={
                  mode === "register" ? "new-password" : "current-password"
                }
                onChange={(e) => setPassword(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void submit();
                }}
              />
            </label>
            {error && <div className="app-error">{error}</div>}
            <div className="retention-actions">
              <button onClick={onClose}>Close</button>
              <button
                className="primary"
                disabled={busy || !url.trim() || !email.trim() || !password}
                onClick={() => void submit()}
              >
                {busy
                  ? "…"
                  : mode === "register"
                    ? "Create account"
                    : "Sign in"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
