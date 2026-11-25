import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";

let cached: string | null = null;

/** The app version from tauri.conf.json, so the UI never shows a stale hardcoded one. */
export function useAppVersion(): string {
  const [version, setVersion] = useState<string>(cached ?? "");
  useEffect(() => {
    if (cached) return;
    getVersion()
      .then((v) => {
        cached = v;
        setVersion(v);
      })
      .catch(() => {});
  }, []);
  return version;
}
