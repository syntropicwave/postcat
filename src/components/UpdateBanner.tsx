import { useUpdater } from "../state/updater";
import { Icon } from "./Icon";

/**
 * Non-intrusive banner (bottom-right) shown when an update is available or
 * installing. Manual "check now" feedback lives in Settings instead.
 */
export function UpdateBanner() {
  const { status, version, notes, progress, dismissed, install, dismiss } =
    useUpdater();

  const busy = status === "downloading" || status === "installing";
  if (
    !(status === "available" || busy) ||
    (status === "available" && dismissed)
  )
    return null;

  return (
    <div className="update-banner">
      <div className="update-banner-head">
        <Icon name="download" size={16} />
        <span className="update-banner-title">
          {busy ? "Updating postcat…" : `postcat ${version} is available`}
        </span>
        {!busy && (
          <button className="update-banner-x" title="Later" onClick={dismiss}>
            <Icon name="x" size={14} />
          </button>
        )}
      </div>

      {status === "available" && notes && (
        <div className="update-banner-notes">{notes}</div>
      )}

      {busy ? (
        <div className="update-progress">
          <div
            className="update-progress-bar"
            style={{ width: `${Math.round(progress * 100)}%` }}
          />
          <span className="update-progress-label">
            {status === "installing"
              ? "Installing — the app will restart…"
              : progress > 0
                ? `${Math.round(progress * 100)}%`
                : "Downloading…"}
          </span>
        </div>
      ) : (
        <div className="update-banner-actions">
          <button onClick={dismiss}>Later</button>
          <button className="primary" onClick={() => void install()}>
            Install &amp; restart
          </button>
        </div>
      )}
    </div>
  );
}
