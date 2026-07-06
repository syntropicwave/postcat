import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { appSettingsGet } from "../ipc/commands";
import { useAppSettings } from "../state/appSettings";
import type { AppSettings } from "../types";

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const stored = useAppSettings((s) => s.settings);
  const update = useAppSettings((s) => s.update);
  const [s, setS] = useState<AppSettings | null>(stored);

  useEffect(() => {
    if (!s) appSettingsGet().then(setS);
  }, [s]);

  if (!s) return null;
  const set = (p: Partial<AppSettings>) => setS({ ...s, ...p });

  const save = async () => {
    await update(s);
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal settings-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="retention-title">Settings</div>

        <div className="settings-body">
          {/* ---------------- Appearance ---------------- */}
          <div className="settings-section">Appearance</div>

          <label className="modal-field">
            Theme
            <select
              value={s.theme}
              onChange={(e) =>
                set({ theme: e.target.value as AppSettings["theme"] })
              }
            >
              <option value="system">System</option>
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select>
          </label>

          <label className="modal-field">
            Response pane
            <select
              value={s.response_layout}
              onChange={(e) =>
                set({
                  response_layout: e.target
                    .value as AppSettings["response_layout"],
                })
              }
            >
              <option value="bottom">Below the request</option>
              <option value="right">Right of the request</option>
            </select>
          </label>

          <label className="modal-field">
            Editor font size (px)
            <input
              type="number"
              min={10}
              max={22}
              value={s.editor_font_size}
              onChange={(e) =>
                set({
                  editor_font_size: clampNum(Number(e.target.value), 10, 22),
                })
              }
            />
          </label>

          <label className="modal-check">
            <input
              type="checkbox"
              checked={s.wrap_response}
              onChange={(e) => set({ wrap_response: e.target.checked })}
            />
            Wrap long response bodies by default
          </label>

          {/* ---------------- Request defaults ---------------- */}
          <div className="settings-section">Request defaults</div>
          <div className="settings-hint">
            Applied to newly created requests. Existing tabs keep their own
            per-request settings.
          </div>

          <label className="modal-field">
            Timeout (ms, 0 = none)
            <input
              type="number"
              min={0}
              value={s.default_timeout_ms}
              onChange={(e) =>
                set({ default_timeout_ms: Math.max(0, Number(e.target.value)) })
              }
            />
          </label>

          <label className="modal-check">
            <input
              type="checkbox"
              checked={s.default_verify_ssl}
              onChange={(e) => set({ default_verify_ssl: e.target.checked })}
            />
            Verify SSL certificates
          </label>

          <label className="modal-check">
            <input
              type="checkbox"
              checked={s.default_follow_redirects}
              onChange={(e) =>
                set({ default_follow_redirects: e.target.checked })
              }
            />
            Automatically follow redirects
          </label>

          {s.default_follow_redirects && (
            <label className="modal-field">
              Maximum redirects
              <input
                type="number"
                min={0}
                value={s.default_max_redirects}
                onChange={(e) =>
                  set({
                    default_max_redirects: Math.max(0, Number(e.target.value)),
                  })
                }
              />
            </label>
          )}

          <label className="modal-field">
            Response capture limit (KB)
            <input
              type="number"
              min={64}
              value={s.max_captured_body_kb}
              onChange={(e) =>
                set({ max_captured_body_kb: Number(e.target.value) })
              }
            />
          </label>

          {/* ---------------- Network ---------------- */}
          <div className="settings-section">Network &amp; certificates</div>

          <label className="modal-field">
            Proxy
            <select
              value={s.proxy_mode}
              onChange={(e) => set({ proxy_mode: e.target.value })}
            >
              <option value="system">System / environment</option>
              <option value="none">No proxy</option>
              <option value="custom">Custom</option>
            </select>
          </label>
          {s.proxy_mode === "custom" && (
            <label className="modal-field">
              Proxy URL
              <input
                value={s.proxy_url}
                placeholder="http://127.0.0.1:8888"
                onChange={(e) => set({ proxy_url: e.target.value })}
              />
            </label>
          )}

          <label className="modal-field">
            Extra CA certificates (PEM)
            <div className="cert-row">
              <input
                readOnly
                value={s.ca_cert_paths.join("; ")}
                placeholder="none"
              />
              <button
                onClick={async () => {
                  const path = await open({
                    multiple: false,
                    filters: [
                      { name: "PEM", extensions: ["pem", "crt", "cer"] },
                    ],
                  });
                  if (typeof path === "string")
                    set({ ca_cert_paths: [...s.ca_cert_paths, path] });
                }}
              >
                Add…
              </button>
              {s.ca_cert_paths.length > 0 && (
                <button onClick={() => set({ ca_cert_paths: [] })}>
                  Clear
                </button>
              )}
            </div>
          </label>

          <label className="modal-field">
            Client certificate (PKCS#12 / .pfx)
            <div className="cert-row">
              <input readOnly value={s.client_cert_path} placeholder="none" />
              <button
                onClick={async () => {
                  const path = await open({
                    multiple: false,
                    filters: [{ name: "PKCS#12", extensions: ["pfx", "p12"] }],
                  });
                  if (typeof path === "string") set({ client_cert_path: path });
                }}
              >
                Pick…
              </button>
              {s.client_cert_path && (
                <button onClick={() => set({ client_cert_path: "" })}>
                  Clear
                </button>
              )}
            </div>
          </label>
          {s.client_cert_path && (
            <label className="modal-field">
              Certificate passphrase
              <input
                type="password"
                value={s.client_cert_password}
                onChange={(e) => set({ client_cert_password: e.target.value })}
              />
            </label>
          )}
        </div>

        <div className="retention-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="primary" onClick={() => void save()}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

function clampNum(v: number, lo: number, hi: number): number {
  if (Number.isNaN(v)) return lo;
  return Math.min(Math.max(v, lo), hi);
}
