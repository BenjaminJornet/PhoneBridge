import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getAdapterRegistry, diagnoseAdb, scanBackupSources, scanSmartSwitchCategories } from "../../lib/api";
import type { SyncState, ConsolidationProgress, SyncProgressPayload, AdbPullProgressPayload } from "./useSyncState";

function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function useSyncProgress(state: SyncState) {
  const {
    sources,
    selectedSourceId,
    selectedSourcePath,
    setSources,
    setAdapterRegistry,
    setAdbDiagnostic,
    setSelectedSourceId,
    setSelectedSourcePath,
    setCategories,
    setSelectedCategories,
    setProgress,
    setSyncProgress,
    setAdbPullProgress,
    setStatus,
    setStatusTone,
  } = state;

  useEffect(() => {
    let cancelled = false;

    Promise.all([scanBackupSources(), getAdapterRegistry(), diagnoseAdb()])
      .then(([backupSources, registry, diagnostic]) => {
        if (!cancelled) {
          const nextSources = backupSources;
          setAdapterRegistry(registry);
          setAdbDiagnostic(diagnostic);
          setSources(nextSources);
          const firstSmartSwitch = nextSources.find((source) => source.adapter === "samsung-smartswitch" && source.path);
          if (firstSmartSwitch?.path) {
            setSelectedSourceId(firstSmartSwitch.id);
            setSelectedSourcePath(firstSmartSwitch.path);
          }
        }
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setStatus(cause instanceof Error ? cause.message : String(cause));
          setStatusTone("warning");
        }
      });

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    listen<ConsolidationProgress>("consolidation-progress", (event) => {
      if (!cancelled) {
        setProgress(event.payload);
      }
    }).then((nextUnlisten) => {
      if (cancelled) {
        nextUnlisten();
      } else {
        unlisten = nextUnlisten;
      }
    });
    listen<SyncProgressPayload>("smartswitch-sync-progress", (event) => {
      if (!cancelled) {
        setSyncProgress(event.payload);
      }
    }).then((nextUnlisten) => {
      const previous = unlisten;
      if (cancelled) {
        nextUnlisten();
      } else {
        unlisten = () => {
          previous?.();
          nextUnlisten();
        };
      }
    });
    listen<AdbPullProgressPayload>("adb-pull-progress", (event) => {
      if (!cancelled) {
        setAdbPullProgress(event.payload);
      }
    }).then((nextUnlisten) => {
      const previous = unlisten;
      if (cancelled) {
        nextUnlisten();
      } else {
        unlisten = () => {
          previous?.();
          nextUnlisten();
        };
      }
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!selectedSourcePath) {
      setCategories([]);
      setSelectedCategories([]);
      return;
    }

    let cancelled = false;
    const selectedSource = sources.find((source) => source.id === selectedSourceId || source.path === selectedSourcePath);
    if (selectedSource?.adapter !== "samsung-smartswitch") {
      setCategories([]);
      setSelectedCategories([]);
      return;
    }

    setStatus("Scanning SmartSwitch categories...");
    scanSmartSwitchCategories(selectedSourcePath)
      .then((nextCategories) => {
        if (!cancelled) {
          setCategories(nextCategories);
          setSelectedCategories(nextCategories.map((category) => category.name));
          setStatus("Ready");
        }
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setStatus(cause instanceof Error ? cause.message : String(cause));
        }
      });

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedSourceId, selectedSourcePath, sources]);
}
