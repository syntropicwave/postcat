import { useEffect, useState } from "react";
import { oauth2Authorize, oauth2FetchToken } from "../ipc/commands";
import type { AuthSpec, OAuth2Config } from "../types";
import { EMPTY_OAUTH2 } from "../types";

interface Props {
  auth: AuthSpec;
  onChange: (auth: AuthSpec) => void;
  /** Whether the "Inherit" option makes sense (request inside a collection). */
  allowInherit: boolean;
}

export function AuthEditor({ auth, onChange, allowInherit }: Props) {
  const setKind = (kind: string) => {
    switch (kind) {
      case "none":
        onChange({ kind: "none" });
        break;
      case "inherit":
        onChange({ kind: "inherit" });
        break;
      case "api_key":
        onChange({
          kind: "api_key",
          key: "X-Api-Key",
          value: "",
          in_query: false,
        });
        break;
      case "bearer":
        onChange({ kind: "bearer", token: "" });
        break;
      case "basic":
        onChange({ kind: "basic", username: "", password: "" });
        break;
      case "oauth2":
        onChange({ kind: "oauth2", ...EMPTY_OAUTH2 });
        break;
      case "aws_sig_v4":
        onChange({
          kind: "aws_sig_v4",
          access_key: "",
          secret_key: "",
          region: "us-east-1",
          service: "execute-api",
          session_token: "",
        });
        break;
    }
  };

  return (
    <div className="auth-editor">
      <div className="auth-kind">
        <select value={auth.kind} onChange={(e) => setKind(e.target.value)}>
          <option value="none">No auth</option>
          {allowInherit && <option value="inherit">Inherit from parent</option>}
          <option value="api_key">API Key</option>
          <option value="bearer">Bearer token</option>
          <option value="basic">Basic auth</option>
          <option value="oauth2">OAuth 2.0</option>
          <option value="aws_sig_v4">AWS Signature v4</option>
        </select>
        <span className="auth-hint">
          Values support {"{{variables}}"}; secrets are masked in history.
        </span>
      </div>

      {auth.kind === "api_key" && (
        <div className="auth-fields">
          <Field
            label="Key"
            value={auth.key}
            onChange={(key) => onChange({ ...auth, key })}
          />
          <Field
            label="Value"
            value={auth.value}
            onChange={(value) => onChange({ ...auth, value })}
          />
          <label className="auth-check">
            <input
              type="checkbox"
              checked={auth.in_query}
              onChange={(e) =>
                onChange({ ...auth, in_query: e.target.checked })
              }
            />
            add to query string instead of headers
          </label>
        </div>
      )}

      {auth.kind === "bearer" && (
        <div className="auth-fields">
          <Field
            label="Token"
            value={auth.token}
            onChange={(token) => onChange({ ...auth, token })}
          />
        </div>
      )}

      {auth.kind === "basic" && (
        <div className="auth-fields">
          <Field
            label="Username"
            value={auth.username}
            onChange={(username) => onChange({ ...auth, username })}
          />
          <Field
            label="Password"
            value={auth.password}
            password
            onChange={(password) => onChange({ ...auth, password })}
          />
        </div>
      )}

      {auth.kind === "oauth2" && (
        <OAuth2Editor auth={auth} onChange={onChange} />
      )}

      {auth.kind === "aws_sig_v4" && (
        <div className="auth-fields">
          <Field
            label="Access key"
            value={auth.access_key}
            onChange={(access_key) => onChange({ ...auth, access_key })}
          />
          <Field
            label="Secret key"
            value={auth.secret_key}
            password
            onChange={(secret_key) => onChange({ ...auth, secret_key })}
          />
          <Field
            label="Region"
            value={auth.region}
            onChange={(region) => onChange({ ...auth, region })}
          />
          <Field
            label="Service"
            value={auth.service}
            onChange={(service) => onChange({ ...auth, service })}
          />
          <Field
            label="Session token (optional)"
            value={auth.session_token}
            onChange={(session_token) => onChange({ ...auth, session_token })}
          />
        </div>
      )}
    </div>
  );
}

function OAuth2Editor({
  auth,
  onChange,
}: {
  auth: { kind: "oauth2" } & OAuth2Config;
  onChange: (auth: AuthSpec) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [nowSec, setNowSec] = useState(0);

  // Clock for the "expires in…" label (impure Date.now stays out of render).
  useEffect(() => {
    const tick = () => setNowSec(Math.floor(Date.now() / 1000));
    tick();
    const t = setInterval(tick, 30_000);
    return () => clearInterval(t);
  }, []);

  const patch = (p: Partial<OAuth2Config>) => onChange({ ...auth, ...p });

  const getToken = async () => {
    setBusy(true);
    setError(null);
    try {
      const token =
        auth.grant_type === "authorization_code"
          ? await oauth2Authorize(auth)
          : await oauth2FetchToken(auth);
      patch({
        access_token: token.access_token,
        refresh_token: token.refresh_token || auth.refresh_token,
        expires_at: token.expires_at,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const tokenStatus = () => {
    if (!auth.access_token) return "no token yet";
    if (auth.expires_at === 0 || nowSec === 0) return "token acquired";
    const left = auth.expires_at - nowSec;
    if (left <= 0)
      return auth.refresh_token
        ? "expired — will auto-refresh on send"
        : "expired";
    return `expires in ${Math.floor(left / 60)} min`;
  };

  return (
    <div className="auth-fields">
      <label className="modal-field">
        Grant type
        <select
          value={auth.grant_type}
          onChange={(e) => patch({ grant_type: e.target.value })}
        >
          <option value="client_credentials">Client credentials</option>
          <option value="password">Password credentials</option>
          <option value="authorization_code">
            Authorization code (PKCE, opens browser)
          </option>
        </select>
      </label>
      {auth.grant_type === "authorization_code" && (
        <Field
          label="Auth URL"
          value={auth.auth_url}
          onChange={(auth_url) => patch({ auth_url })}
        />
      )}
      <Field
        label="Access token URL"
        value={auth.token_url}
        onChange={(token_url) => patch({ token_url })}
      />
      <Field
        label="Client ID"
        value={auth.client_id}
        onChange={(client_id) => patch({ client_id })}
      />
      <Field
        label="Client secret"
        value={auth.client_secret}
        password
        onChange={(client_secret) => patch({ client_secret })}
      />
      <Field
        label="Scope"
        value={auth.scope}
        onChange={(scope) => patch({ scope })}
      />
      {auth.grant_type === "password" && (
        <>
          <Field
            label="Username"
            value={auth.username}
            onChange={(username) => patch({ username })}
          />
          <Field
            label="Password"
            value={auth.password}
            password
            onChange={(password) => patch({ password })}
          />
        </>
      )}
      <label className="auth-check">
        <input
          type="checkbox"
          checked={auth.credentials_in_body}
          onChange={(e) => patch({ credentials_in_body: e.target.checked })}
        />
        send client credentials in body (instead of Basic header)
      </label>

      <div className="oauth-actions">
        <button
          className="primary"
          disabled={busy}
          onClick={() => void getToken()}
        >
          {busy ? "Waiting…" : "Get new access token"}
        </button>
        <span className="oauth-status">{tokenStatus()}</span>
      </div>
      {error && <div className="app-error">{error}</div>}
    </div>
  );
}

function Field({
  label,
  value,
  password,
  onChange,
}: {
  label: string;
  value: string;
  password?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <label className="modal-field">
      {label}
      <input
        type={password ? "password" : "text"}
        value={value}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}
