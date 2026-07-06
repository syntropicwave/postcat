import { useEffect, useState } from "react";
import { useAppSettings } from "../state/appSettings";

/**
 * Whether the UI should render dark. Follows the OS unless the user forced a
 * theme in Settings (system / light / dark). Drives CodeMirror themes.
 */
export function usePrefersDark(): boolean {
  const theme = useAppSettings((s) => s.settings?.theme ?? "system");
  const [osDark, setOsDark] = useState(
    () => window.matchMedia("(prefers-color-scheme: dark)").matches,
  );

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) => setOsDark(e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  if (theme === "dark") return true;
  if (theme === "light") return false;
  return osDark;
}
