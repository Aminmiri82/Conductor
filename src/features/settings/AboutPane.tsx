import { useCallback, useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import type { Update } from "@tauri-apps/plugin-updater";
import { Button } from "@/components/ui/button";
import { SectionTitle } from "./SettingsParts";

export function AboutPane({
  updateCheckRequested,
  consumeUpdateCheck,
}: {
  updateCheckRequested: boolean;
  consumeUpdateCheck: () => void;
}) {
  const [version, setVersion] = useState("");
  const [busy, setBusy] = useState(false);
  const [availableVersion, setAvailableVersion] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const updateRef = useRef<Update | null>(null);
  const mountedRef = useRef(true);
  const busyRef = useRef(false);

  useEffect(() => {
    mountedRef.current = true;
    void getVersion().then((value) => {
      if (mountedRef.current) setVersion(value);
    });
    return () => {
      mountedRef.current = false;
      void updateRef.current?.close();
      updateRef.current = null;
    };
  }, []);

  const checkForUpdates = useCallback(async () => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setMessage("");
    setAvailableVersion(null);
    try {
      await updateRef.current?.close();
      updateRef.current = null;
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      if (!mountedRef.current) {
        await update?.close();
        return;
      }
      updateRef.current = update;
      if (update) {
        setAvailableVersion(update.version);
        setMessage(`Conductor ${update.version} is available.`);
      } else {
        setMessage("You have the latest release.");
      }
    } catch (error) {
      if (mountedRef.current) {
        setMessage(`Could not check for updates: ${String(error)}`);
      }
    } finally {
      busyRef.current = false;
      if (mountedRef.current) setBusy(false);
    }
  }, []);

  useEffect(() => {
    if (!updateCheckRequested) return;
    consumeUpdateCheck();
    void checkForUpdates();
  }, [updateCheckRequested, consumeUpdateCheck, checkForUpdates]);

  async function installUpdate() {
    const update = updateRef.current;
    if (!update) return;
    busyRef.current = true;
    setBusy(true);
    setMessage(`Installing Conductor ${update.version}...`);
    try {
      await update.downloadAndInstall();
    } catch (error) {
      setMessage(`Could not install the update: ${String(error)}`);
      busyRef.current = false;
      setBusy(false);
      return;
    }
    updateRef.current = null;
    setAvailableVersion(null);
    setMessage(`Conductor ${update.version} is installed. Restart to use it.`);
    try {
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch {
      busyRef.current = false;
      setBusy(false);
    }
  }

  return (
    <div>
      <SectionTitle
        title="Conductor"
        sub="API workspace for collections and requests."
      />
      <div className="app-mono text-xs leading-6 text-[var(--app-dim)]">
        Version {version}
      </div>
      <div className="mt-5 flex items-center gap-3">
        <Button disabled={busy} onClick={() => void checkForUpdates()}>
          {busy && !availableVersion ? "Checking..." : "Check for updates"}
        </Button>
        {availableVersion ? (
          <Button disabled={busy} onClick={() => void installUpdate()}>
            {busy ? "Installing..." : "Install and restart"}
          </Button>
        ) : null}
      </div>
      {message ? (
        <p className="mt-3 text-xs text-[var(--app-dim)]" role="status">
          {message}
        </p>
      ) : null}
    </div>
  );
}
