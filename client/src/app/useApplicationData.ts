import { currentSession } from "../lib/webSession";
import { useCallback, useEffect, useRef, useState } from "react";
import { checkServerVersion } from "../api/account";
import { getSnapshot } from "../api/portfolio";
import { getModelConfig } from "../api/settings";
import { getWatchBoard, getWatchCatalog } from "../api/watch";
import { startAutoSync, type SyncState } from "../lib/autoSync";
import {
  prefetchForSession,
  readSessionCache,
  writeSessionCache,
} from "../lib/sessionCache";
import {
  DATA_SAVED,
  SERVER_SESSION_CHANGED,
  SERVER_DATA_UPDATED,
} from "../lib/syncEvents";
import type { ModelConfig } from "../types/ai";
import type { Snapshot } from "../types/user";
import type { View } from "./navigation";

/** Owns the server-data lifecycle. Pages own their forms; the shell owns navigation. */
export function useApplicationData(view: View) {
  const [session] = useState(currentSession);
  const [snapshot, setSnapshotState] = useState<Snapshot | null>(() =>
    readSessionCache<Snapshot>(session, "snapshot"),
  );
  const [model, setModelState] = useState<ModelConfig | null>(() =>
    readSessionCache<ModelConfig>(session, "modelConfig"),
  );
  const [modelError, setModelError] = useState("");
  const [loading, setLoading] = useState(() => snapshot === null);
  const [startupError, setStartupError] = useState("");
  const [remotePending, setRemotePending] = useState(false);
  const [remoteViewRevision, setRemoteViewRevision] = useState(0);
  const [dataRevision, setDataRevision] = useState(0);
  const [syncState, setSyncState] = useState<SyncState>({
    phase: "waiting",
    message: "正在检查同步状态",
  });
  const generation = useRef(0);
  const hasSnapshot = useRef(snapshot !== null);
  hasSnapshot.current = snapshot !== null;
  const setSnapshot = useCallback(
    (next: Snapshot) => {
      // A saved response supersedes every refresh that started before it.
      generation.current += 1;
      setSnapshotState(next);
      writeSessionCache(session, "snapshot", next);
    },
    [session],
  );
  const setModel = useCallback(
    (next: ModelConfig | null) => {
      setModelState(next);
      if (next) writeSessionCache(session, "modelConfig", next);
    },
    [session],
  );
  const edited = useRef(new Set<string>());
  const currentView = useRef(view);
  currentView.current = view;
  const syncController = useRef<ReturnType<typeof startAutoSync> | null>(null);
  const hasDraft = useCallback(() => edited.current.size > 0, []);
  const markEdited = useCallback((scope = "page") => {
    edited.current.add(scope);
  }, []);
  const clearDraft = useCallback((scope?: string) => {
    if (scope === undefined) edited.current.clear();
    else edited.current.delete(scope);
  }, []);
  const protectDraft = useCallback(() => {
    if (!edited.current.size || currentView.current === "advisor") return false;
    session.blockStaleWrites(true);
    setRemotePending(true);
    return true;
  }, [session]);

  const loadRemoteData = useCallback(
    async (protectDrafts = false) => {
      if (protectDrafts && protectDraft()) return false;
      const request = ++generation.current;
      const [snapshotResult, modelResult] = await Promise.allSettled([
        getSnapshot(),
        getModelConfig(),
      ]);
      if (request !== generation.current) return false;
      if (snapshotResult.status === "rejected") throw snapshotResult.reason;
      const next = snapshotResult.value;
      const nextModel =
        modelResult.status === "fulfilled" ? modelResult.value : undefined;
      const nextModelError =
        modelResult.status === "rejected" ? String(modelResult.reason) : "";
      // Input can change while either request is in flight.
      if (protectDrafts && protectDraft()) return false;
      if (nextModel) setModel(nextModel);
      setModelError(nextModelError);
      setSnapshotState(next);
      writeSessionCache(session, "snapshot", next);
      setRemoteViewRevision((value) => value + 1);
      setRemotePending(false);
      session.blockStaleWrites(false);
      window.dispatchEvent(new Event(SERVER_DATA_UPDATED));
      return true;
    },
    [protectDraft, session, setModel],
  );

  const loadApplication = useCallback(() => {
    if (!hasSnapshot.current) setLoading(true);
    setStartupError("");
    const request = ++generation.current;
    // Start initial route reads in parallel with snapshot instead of waiting for mount.
    if (currentView.current === "watch") {
      void prefetchForSession(session, "watch:board", getWatchBoard).catch(
        () => {},
      );
      void prefetchForSession(session, "watch:catalog", getWatchCatalog).catch(
        () => {},
      );
    }
    getSnapshot()
      .then((next) => {
        if (request === generation.current) {
          setSnapshotState(next);
          writeSessionCache(session, "snapshot", next);
        }
      })
      .catch((error) => {
        if (request === generation.current && !hasSnapshot.current)
          setStartupError(String(error));
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
  }, [session, setModel]);

  useEffect(() => {
    loadApplication();
    return () => {
      generation.current += 1;
      if (!session.signal.aborted) session.blockStaleWrites(false);
    };
  }, [loadApplication, session]);

  useEffect(() => {
    if (loading || startupError) return;
    const controller = startAutoSync({
      session,
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
  }, [loading, startupError, loadRemoteData, session]);

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
