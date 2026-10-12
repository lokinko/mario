import { useCallback, useEffect, useRef, useState } from "react";
import { checkServerVersion } from "../api/account";
import { getSnapshot } from "../api/portfolio";
import { getModelConfig } from "../api/settings";
import { startAutoSync, type SyncState } from "../lib/autoSync";
import {
  blockStaleWrites,
  DATA_SAVED,
  SERVER_SESSION_CHANGED,
  SERVER_DATA_UPDATED,
} from "../lib/syncEvents";
import type { ModelConfig, Snapshot } from "../types";
import type { View } from "./navigation";

/** Owns the server-data lifecycle. Pages own their forms; the shell owns navigation. */
export function useApplicationData(view: View) {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [model, setModel] = useState<ModelConfig | null>(null);
  const [modelError, setModelError] = useState("");
  const [loading, setLoading] = useState(true);
  const [startupError, setStartupError] = useState("");
  const [remotePending, setRemotePending] = useState(false);
  const [remoteViewRevision, setRemoteViewRevision] = useState(0);
  const [dataRevision, setDataRevision] = useState(0);
  const [syncState, setSyncState] = useState<SyncState>({
    phase: "waiting",
    message: "正在检查同步状态",
  });
  const generation = useRef(0);
  const edited = useRef(false);
  const currentView = useRef(view);
  currentView.current = view;
  const syncController = useRef<ReturnType<typeof startAutoSync> | null>(null);
  const hasDraft = useCallback(() => edited.current, []);
  const markEdited = useCallback(() => {
    edited.current = true;
  }, []);
  const clearDraft = useCallback(() => {
    edited.current = false;
  }, []);
  const protectDraft = useCallback(() => {
    if (!edited.current || ["advisor", "cloud"].includes(currentView.current))
      return false;
    blockStaleWrites(true);
    setRemotePending(true);
    return true;
  }, []);

  const loadRemoteData = useCallback(
    async (protectDrafts = false) => {
      if (protectDrafts && protectDraft()) return false;
      const request = ++generation.current;
      const next = await getSnapshot();
      if (request !== generation.current) return false;
      let nextModel: ModelConfig | undefined;
      let nextModelError = "";
      try {
        nextModel = await getModelConfig();
      } catch (error) {
        nextModelError = String(error);
      }
      if (request !== generation.current) return false;
      // Input can change while either request is in flight.
      if (protectDrafts && protectDraft()) return false;
      if (nextModel) setModel(nextModel);
      setModelError(nextModelError);
      setSnapshot(next);
      setRemoteViewRevision((value) => value + 1);
      setRemotePending(false);
      blockStaleWrites(false);
      window.dispatchEvent(new Event(SERVER_DATA_UPDATED));
      return true;
    },
    [protectDraft],
  );

  const loadApplication = useCallback(() => {
    setLoading(true);
    setStartupError("");
    const request = ++generation.current;
    getSnapshot()
      .then((next) => {
        if (request === generation.current) setSnapshot(next);
      })
      .catch((error) => {
        if (request === generation.current) setStartupError(String(error));
      })
      .finally(() => {
        if (request === generation.current) setLoading(false);
      });
    getModelConfig()
      .then((next) => {
        if (request === generation.current) {
          setModel(next);
          setModelError("");
        }
      })
      .catch((error) => {
        if (request === generation.current) setModelError(String(error));
      });
  }, []);

  useEffect(() => {
    loadApplication();
    return () => {
      generation.current += 1;
      blockStaleWrites(false);
    };
  }, [loadApplication]);

  useEffect(() => {
    if (loading || startupError) return;
    const controller = startAutoSync({
      sync: checkServerVersion,
      visible: () =>
        document.visibilityState !== "hidden" && document.hasFocus(),
      online: () => navigator.onLine,
      onStatus: setSyncState,
      onRemoteUpdate: async () => {
        await loadRemoteData(true);
      },
    });
    syncController.current = controller;
    window.addEventListener(DATA_SAVED, controller.changed);
    window.addEventListener(SERVER_SESSION_CHANGED, controller.retry);
    for (const event of ["focus", "blur", "online", "offline"])
      window.addEventListener(event, controller.wake);
    document.addEventListener("visibilitychange", controller.wake);
    return () => {
      controller.dispose();
      syncController.current = null;
      window.removeEventListener(DATA_SAVED, controller.changed);
      window.removeEventListener(SERVER_SESSION_CHANGED, controller.retry);
      for (const event of ["focus", "blur", "online", "offline"])
        window.removeEventListener(event, controller.wake);
      document.removeEventListener("visibilitychange", controller.wake);
    };
  }, [loading, startupError, loadRemoteData]);

  const acceptRemoteData = useCallback(async () => {
    if (await loadRemoteData()) clearDraft();
  }, [loadRemoteData, clearDraft]);
  const refreshInvestmentData = useCallback(async () => {
    if (await loadRemoteData()) {
      clearDraft();
      setDataRevision((value) => value + 1);
    }
  }, [loadRemoteData, clearDraft]);

  return {
    snapshot,
    setSnapshot,
    model,
    setModel,
    modelError,
    loading,
    startupError,
    remotePending,
    remoteViewRevision,
    dataRevision,
    syncState,
    loadApplication,
    loadRemoteData,
    acceptRemoteData,
    refreshInvestmentData,
    hasDraft,
    markEdited,
    clearDraft,
    retrySync: () => syncController.current?.retry(),
  };
}
