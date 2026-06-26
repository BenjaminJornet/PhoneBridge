import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import PathPickerField from "../components/PathPickerField";
import SectionHeader from "../components/SectionHeader";
import StatusCallout from "../components/StatusCallout";
import {
  getAdapterRegistry,
  diagnoseAdb,
  indexMultimedia,
  listBackupCoverage,
  planConsolidation,
  previewDeviceMedia,
  cancelPullFromDevice,
  pullFromDevice,
  runSmartSwitchSync,
  runConsolidation,
  scanBackupSources,
  scanSmartSwitchCategories,
} from "../lib/api";
import { formatBytes, formatCount } from "../lib/format";
import type {
  BackupCoverage,
  BackupSource,
  AdapterDefinition,
  AdbDiagnostic,
  AdbMediaFolderPreview,
  AdbPullResult,
  ConsolidationPlan,
  ConsolidationResult,
  IndexSummary,
  SmartSwitchCategory,
  SmartSwitchSyncResult,
} from "../lib/types";
import type { StatusTone } from "../lib/ux";

function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

interface SyncProps {
  onNavigate?: (page: "gallery") => void;
}

export default function Sync({ onNavigate }: SyncProps) {
  const [sources, setSources] = useState<BackupSource[]>([]);
  const [adapterRegistry, setAdapterRegistry] = useState<AdapterDefinition[]>([]);
  const [selectedSourceId, setSelectedSourceId] = useState("");
  const [selectedSourcePath, setSelectedSourcePath] = useState("");
  const [categories, setCategories] = useState<SmartSwitchCategory[]>([]);
  const [selectedCategories, setSelectedCategories] = useState<string[]>([]);
  const [destinationPath, setDestinationPath] = useState("~/.phonebridge/library");
  const [summary, setSummary] = useState<IndexSummary | null>(null);
  const [syncResult, setSyncResult] = useState<SmartSwitchSyncResult | null>(null);
  const [adbPullResult, setAdbPullResult] = useState<AdbPullResult | null>(null);
  const [adbDiagnostic, setAdbDiagnostic] = useState<AdbDiagnostic | null>(null);
  const [mediaPreview, setMediaPreview] = useState<AdbMediaFolderPreview[] | null>(null);
  const [selectedPullKeys, setSelectedPullKeys] = useState<string[]>([]);
  const [consolidationPlan, setConsolidationPlan] = useState<ConsolidationPlan | null>(null);
  const [consolidationResult, setConsolidationResult] = useState<ConsolidationResult | null>(null);
  const [backupCoverage, setBackupCoverage] = useState<BackupCoverage[]>([]);
  const [progress, setProgress] = useState<{ processedFiles: number; totalFiles: number; currentPath: string } | null>(null);
  const [syncProgress, setSyncProgress] = useState<{ copiedFiles: number; skippedFiles: number; currentPath: string } | null>(null);
  const [adbPullProgress, setAdbPullProgress] = useState<{ pulledPaths: number; skippedPaths: number; pulledFiles: number; skippedFiles: number; permissionDeniedFiles: number; totalFiles: number; currentPath: string } | null>(null);
  const [status, setStatus] = useState("Ready");
  const [statusTone, setStatusTone] = useState<StatusTone>("info");
  const [showAdvancedTools, setShowAdvancedTools] = useState(false);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [step, setStep] = useState<1 | 2 | 3 | 4>(1);

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
  }, []);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    listen<{ processedFiles: number; totalFiles: number; currentPath: string }>("consolidation-progress", (event) => {
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
    listen<{ copiedFiles: number; skippedFiles: number; currentPath: string }>("smartswitch-sync-progress", (event) => {
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
    listen<{ pulledPaths: number; skippedPaths: number; pulledFiles: number; skippedFiles: number; permissionDeniedFiles: number; totalFiles: number; currentPath: string }>("adb-pull-progress", (event) => {
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
  }, [selectedSourceId, selectedSourcePath, sources]);

  async function handleIndexMultimedia() {
    if (!selectedSourcePath) {
      setStatus("Choose a source folder before indexing.");
      return;
    }

    setBusyAction("index");
    setStatus("Indexing selected folder...");
    setStatusTone("info");
    try {
      const nextSummary = await indexMultimedia(selectedSourcePath);
      setSummary(nextSummary);
      setStatus("Index complete");
      setStatusTone("success");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRunSmartSwitchSync() {
    if (!selectedSourcePath || selectedCategories.length === 0) {
      setStatus("Select a SmartSwitch source and at least one category.");
      return;
    }

    setBusyAction("smartswitch-sync");
    setStatus("Synchronizing SmartSwitch backup without deleting or overwriting files...");
    setStatusTone("info");
    setSyncResult(null);
    setSyncProgress(null);
    try {
      const result = await runSmartSwitchSync({
        sourcePath: selectedSourcePath,
        destinationPath,
        categories: selectedCategories,
      });
      setSyncResult(result);
      setSyncProgress(null);
      setStatus("SmartSwitch sync complete");
      setStatusTone("success");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  async function handlePreviewDeviceMedia() {
    const selectedSource = sources.find((source) => source.id === selectedSourceId);
    if (!selectedSource || selectedSource.adapter !== "adb-generic") {
      setStatus("Select an authorized ADB device first.");
      return;
    }

    setBusyAction("adb-preview");
    setStatus("Measuring media on the phone (read-only)...");
    setStatusTone("info");
    try {
      const preview = await previewDeviceMedia(selectedSource.id);
      setMediaPreview(preview);
      setSelectedPullKeys(preview.filter((folder) => folder.available).map((folder) => folder.key));
      const total = preview.reduce((sum, folder) => sum + folder.totalBytes, 0);
      setStatus(`Found ${formatBytes(total)} across ${preview.filter((f) => f.available).length} folders. Pick what to copy.`);
      setStatusTone("success");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  function togglePullKey(key: string) {
    setSelectedPullKeys((current) =>
      current.includes(key) ? current.filter((item) => item !== key) : [...current, key],
    );
  }

  async function handlePullFromDevice() {
    const selectedSource = sources.find((source) => source.id === selectedSourceId);
    if (!selectedSource || selectedSource.adapter !== "adb-generic") {
      setStatus("Select an authorized ADB device before pulling media.");
      return;
    }

    // A selection is only enforced once the user has previewed; before that we keep the
    // legacy behaviour (pull everything) so the primary button still works.
    const keys = mediaPreview ? selectedPullKeys : undefined;
    if (mediaPreview && keys && keys.length === 0) {
      setStatus("Select at least one folder to copy.");
      setStatusTone("warning");
      return;
    }

    setBusyAction("adb-pull");
    setStatus("Pulling media from the Android device without modifying it...");
    setStatusTone("info");
    setAdbPullResult(null);
    setAdbPullProgress(null);
    try {
      const result = await pullFromDevice(selectedSource.id, "~/.phonebridge/staging", keys);
      setAdbPullResult(result);
      setAdbPullProgress(null);
      setSelectedSourcePath(result.sourcePath);
      if (result.cancelled) {
        setStatus(`Copy stopped — ${result.pulledFiles} file${result.pulledFiles === 1 ? "" : "s"} staged. You can still preview and import what was copied.`);
        setStatusTone("warning");
      } else {
        setStatus("Phone media copied into a temporary import folder. Preview it next.");
        setStatusTone("success");
      }
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  function consolidationConfig() {
    const selectedSource = sources.find((source) => source.id === selectedSourceId || source.path === selectedSourcePath);
    return {
      sourcePath: selectedSourcePath,
      destinationPath,
      adapter: selectedSource?.adapter ?? "generic-folder",
      label: selectedSource?.label ?? "User-selected folder",
      deviceId: selectedSource?.device?.id,
      deviceLabel: selectedSource?.label,
    };
  }

  function selectSource(source: BackupSource) {
    setSelectedSourceId(source.id);
    setSelectedSourcePath(source.path ?? "");
    setConsolidationPlan(null);
    setConsolidationResult(null);
    setStep(2);
  }

  function adapterLabel(adapterId: string) {
    return adapterRegistry.find((adapter) => adapter.id === adapterId)?.label ?? adapterId;
  }

  async function handlePlanConsolidation() {
    if (!selectedSourcePath) {
      setStatus("Select a source before planning consolidation.");
      return;
    }

    setBusyAction("preview");
    setStatus("Previewing what will be added to your local library...");
    setStatusTone("info");
    setConsolidationPlan(null);
    setConsolidationResult(null);
    try {
      const plan = await planConsolidation(consolidationConfig());
      setConsolidationPlan(plan);
      setStep(3);
      setStatus("Preview ready. Review the numbers, then import when you are ready.");
      setStatusTone("success");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRunConsolidation() {
    if (!selectedSourcePath) {
      setStatus("Select a source before consolidation.");
      return;
    }

    setBusyAction("import");
    setStatus("Importing and deduplicating without deleting originals...");
    setStatusTone("info");
    setConsolidationResult(null);
    setProgress(null);
    try {
      const result = await runConsolidation(consolidationConfig());
      setConsolidationResult(result);
      setConsolidationPlan(result.plan);
      setBackupCoverage(await listBackupCoverage());
      setProgress(null);
      setStep(4);
      setStatus("Import complete. Your originals were left untouched.");
      setStatusTone("success");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  async function refreshCoverage() {
    try {
      setBackupCoverage(await listBackupCoverage());
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function refreshAdb() {
    setBusyAction("adb-refresh");
    setStatus("Checking Android devices...");
    setStatusTone("info");
    try {
      const [nextSources, diagnostic] = await Promise.all([scanBackupSources(), diagnoseAdb()]);
      setSources(nextSources);
      setAdbDiagnostic(diagnostic);
      const authorized = nextSources.find((source) => source.adapter === "adb-generic");
      if (authorized) {
        selectSource(authorized);
        setStatus("Android phone ready. Click Copy media from phone when you want to import it.");
        setStatusTone("success");
      } else {
        setStatus(diagnostic.message);
        setStatusTone(diagnostic.adbFound ? "warning" : "error");
      }
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  async function chooseDestinationFolder() {
    setStatus("Opening library folder picker...");
    setStatusTone("info");
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setDestinationPath(selected);
      setStatus("Library folder selected.");
      setStatusTone("success");
    } else {
      setStatus("No library folder selected.");
      setStatusTone("warning");
    }
  }

  async function chooseSourceFolder() {
    setStatus("Opening source folder picker...");
    setStatusTone("info");
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      const source = manualSource("generic-folder", selected);
      setSources((current) => upsertSource(current, source));
      selectSource(source);
      setStatus("Folder selected. Preview the import next.");
      setStatusTone("success");
    } else {
      setStatus("No source folder selected.");
      setStatusTone("warning");
    }
  }

  async function chooseSmartSwitchFolder() {
    setStatus("Opening SmartSwitch backup picker...");
    setStatusTone("info");
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") {
      setStatus("No SmartSwitch folder selected.");
      setStatusTone("warning");
      return;
    }

    const source = manualSource("samsung-smartswitch", selected);
    setSources((current) => upsertSource(current, source));
    selectSource(source);
    setStatus("SmartSwitch folder selected. Scanning categories...");
    setStatusTone("info");
    try {
      const nextCategories = await scanSmartSwitchCategories(selected);
      setCategories(nextCategories);
      setSelectedCategories(nextCategories.map((category) => category.name));
      setStatus(nextCategories.length > 0 ? "SmartSwitch categories found. Preview the import next." : "No SmartSwitch media categories found. You can still import this folder generically.");
      setStatusTone(nextCategories.length > 0 ? "success" : "warning");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
      setStatusTone("warning");
    }
  }

  function manualSource(adapter: "generic-folder" | "samsung-smartswitch", path: string): BackupSource {
    return {
      id: `manual-${adapter}:${path}`,
      adapter,
      label: adapter === "samsung-smartswitch" ? "Selected SmartSwitch backup" : "Selected folder",
      path,
    };
  }

  function upsertSource(current: BackupSource[], source: BackupSource): BackupSource[] {
    return [source, ...current.filter((item) => item.id !== source.id)];
  }

  function toggleCategory(category: string) {
    setSelectedCategories((current) =>
      current.includes(category) ? current.filter((item) => item !== category) : [...current, category],
    );
  }

  function selectAllCategories() {
    setSelectedCategories(categories.map((category) => category.name));
  }

  const selectedSource = sources.find((source) => source.id === selectedSourceId || source.path === selectedSourcePath);
  const adbSources = sources.filter((source) => source.adapter === "adb-generic");
  const backupSources = sources.filter((source) => source.path);
  async function handlePrimaryAction() {
    if (selectedSource?.adapter === "adb-generic" && !selectedSourcePath) {
      await handlePullFromDevice();
      return;
    }
    if (!consolidationPlan) {
      await handlePlanConsolidation();
      return;
    }
    await handleRunConsolidation();
  }

  function primaryActionLabel() {
    if (selectedSource?.adapter === "adb-generic" && !selectedSourcePath) {
      return "Copy media from phone";
    }
    if (!selectedSourcePath) {
      return "Choose a source first";
    }
    if (!consolidationPlan) {
      return "Preview import";
    }
    return "Import and deduplicate";
  }

  return (
    <section>
      <SectionHeader
        eyebrow="Guided import"
        title="Choose one source. Preview it. Then import safely."
        description="PhoneBridge keeps originals untouched. It copies data into your local library only after you review what will be added or deduplicated."
      />
      <div className="card" style={{ marginTop: "18px" }}>
        <div className="wizardSteps" aria-label="Import steps">
          <div className={step === 1 ? "step activeStep" : "step"}><span>1</span><p>Choose a source</p></div>
          <div className={step === 2 ? "step activeStep" : "step"}><span>2</span><p>Get the data</p></div>
          <div className={step === 3 ? "step activeStep" : "step"}><span>3</span><p>Preview</p></div>
          <div className={step === 4 ? "step activeStep" : "step"}><span>4</span><p>Done</p></div>
        </div>
        <StatusCallout title="Import status" message={status} tone={statusTone} />

        {/* STEP 1 — Choose a source */}
        {step === 1 && (
          <>
            <h2>Pick what you want to import</h2>
            <div className="sourceTypeGrid">
              <button
                className={selectedSource?.adapter === "adb-generic" ? "sourceTypeCard selectedSource" : "sourceTypeCard"}
                onClick={() => {
                  if (adbSources[0]) {
                    selectSource(adbSources[0]);
                    setStatus("Android phone selected. Measure its media, then copy what you want.");
                    setStatusTone("success");
                  } else {
                    void refreshAdb();
                  }
                }}
                type="button"
              >
                <strong>Android phone</strong>
                <span>{adbSources.length > 0 ? `${adbSources[0].label} ready` : "Check USB / ADB connection"}</span>
                <small>Copies common media folders into a temporary local import folder.</small>
              </button>
              <button
                className={selectedSource?.adapter === "samsung-smartswitch" ? "sourceTypeCard selectedSource" : "sourceTypeCard"}
                onClick={() => {
                  const smartSwitch = backupSources.find((source) => source.adapter === "samsung-smartswitch");
                  if (smartSwitch) {
                    selectSource(smartSwitch);
                    setStatus("SmartSwitch backup selected. Preview the import next.");
                    setStatusTone("success");
                  } else {
                    void chooseSmartSwitchFolder();
                  }
                }}
                type="button"
              >
                <strong>SmartSwitch backup</strong>
                <span>{backupSources.some((source) => source.adapter === "samsung-smartswitch") ? "Backup ready" : "Choose backup folder"}</span>
                <small>Reads media and structured backup categories when available.</small>
              </button>
              <button className="sourceTypeCard" onClick={chooseSourceFolder} type="button">
                <strong>Any folder</strong>
                <span>Photos, videos, music, documents</span>
                <small>Best for exports, copied phone folders, or external drives.</small>
              </button>
            </div>
            <div className="syncActions">
              <button className="pill" disabled={busyAction === "adb-refresh"} onClick={refreshAdb} type="button">
                {busyAction === "adb-refresh" ? "Checking devices..." : "Refresh Android devices"}
              </button>
            </div>
            {sources.length > 0 && (
              <>
                <h2>Detected sources</h2>
                <div className="sourceList">
                  {sources.map((source) => (
                    <button
                      className={source.id === selectedSourceId ? "sourceRow selectedSource" : "sourceRow"}
                      key={source.id}
                      onClick={() => selectSource(source)}
                      type="button"
                    >
                      <strong>{source.label}</strong>
                      <span>{adapterLabel(source.adapter)}</span>
                      {source.device && (
                        <small>
                          {source.device.manufacturer} {source.device.model}
                          {source.device.androidVersion ? ` · Android ${source.device.androidVersion}` : ""}
                        </small>
                      )}
                      {source.path && <small>{source.path}</small>}
                    </button>
                  ))}
                </div>
              </>
            )}
          </>
        )}

        {/* STEP 2 — Get the data */}
        {step === 2 && (
          <>
            <div className="summaryBox">
              <strong>{selectedSource?.label ?? "Selected source"}</strong>
              <span>{selectedSource ? adapterLabel(selectedSource.adapter) : "Choose what to import"}</span>
            </div>
            {selectedSource?.adapter !== "adb-generic" && (
              <PathPickerField
                buttonLabel="Choose source folder"
                description="Only needed when PhoneBridge did not detect your backup automatically."
                label="Selected source"
                onChange={setSelectedSourcePath}
                onChoose={chooseSourceFolder}
                value={selectedSourcePath}
              />
            )}
            {adbDiagnostic && selectedSource?.adapter === "adb-generic" && (
              <div className="summaryBox">
                <strong>{adbDiagnostic.message}</strong>
                <span>{adbDiagnostic.nextAction}</span>
                {adbDiagnostic.adbPath && <small>ADB: {adbDiagnostic.adbPath}</small>}
                {adbDiagnostic.devices.map((device) => (
                  <small key={device.sourceId}>{device.label} · {device.status} · {device.redactedId}</small>
                ))}
              </div>
            )}
            {selectedSource?.adapter === "adb-generic" && (
              <div className="summaryBox">
                <strong>On-device media</strong>
                <span>Measure first so you don&apos;t blindly copy tens of gigabytes. Then pick the folders to import.</span>
                <div className="syncActions">
                  <button className="pill" disabled={busyAction === "adb-preview"} onClick={handlePreviewDeviceMedia} type="button">
                    {busyAction === "adb-preview" ? "Measuring..." : mediaPreview ? "Re-measure phone media" : "Preview phone media"}
                  </button>
                </div>
                {mediaPreview && (
                  <div className="pullFolderList">
                    {mediaPreview.map((folder) => (
                      <label key={folder.key} className={folder.available ? "pullFolderRow" : "pullFolderRow disabledRow"}>
                        <input
                          type="checkbox"
                          checked={selectedPullKeys.includes(folder.key)}
                          disabled={!folder.available}
                          onChange={() => togglePullKey(folder.key)}
                        />
                        <span>{folder.label}</span>
                        <small>{folder.available ? `${formatCount(folder.fileCount)} files · ${formatBytes(folder.totalBytes)}` : "empty / unavailable"}</small>
                      </label>
                    ))}
                    <small>
                      Selected: {formatBytes(
                        mediaPreview
                          .filter((folder) => selectedPullKeys.includes(folder.key))
                          .reduce((sum, folder) => sum + folder.totalBytes, 0),
                      )}
                    </small>
                  </div>
                )}
              </div>
            )}
            {adbPullResult && (
              <div className="summaryBox">
                <strong>{formatCount(adbPullResult.pulledFiles)} files copied from phone · {formatCount(adbPullResult.skippedFiles)} skipped</strong>
                <span>{formatCount(adbPullResult.pulledPaths)} folders scanned · {formatCount(adbPullResult.totalFiles)} files found</span>
                {adbPullResult.permissionDeniedFiles > 0 && <span>{formatCount(adbPullResult.permissionDeniedFiles)} file(s) Android refused to share</span>}
                {adbPullResult.errors.length > 0 && <small>{adbPullResult.errors.length} warning(s): {adbPullResult.errors[0]}</small>}
              </div>
            )}
            {adbPullProgress && (
              <div className="summaryBox">
                <strong>{formatCount(adbPullProgress.pulledFiles)} / {formatCount(adbPullProgress.totalFiles)} files copied</strong>
                <span>{formatCount(adbPullProgress.pulledPaths)} folders done · {formatCount(adbPullProgress.skippedPaths)} skipped</span>
                {adbPullProgress.permissionDeniedFiles > 0 && <span>{formatCount(adbPullProgress.permissionDeniedFiles)} file(s) Android refused to share</span>}
                <span>{adbPullProgress.currentPath}</span>
              </div>
            )}
            {categories.length > 0 && (
              <>
                <h2>SmartSwitch categories</h2>
                <div className="syncActions">
                  <button className="pill" onClick={selectAllCategories} type="button">Select all</button>
                  <button className="pill" onClick={() => setSelectedCategories([])} type="button">Clear</button>
                </div>
                <div className="categoryGrid">
                  {categories.map((category) => (
                    <label className="categoryChoice" key={category.name}>
                      <input
                        checked={selectedCategories.includes(category.name)}
                        onChange={() => toggleCategory(category.name)}
                        type="checkbox"
                      />
                      <span>
                        <strong>{category.name}</strong>
                        <small>{formatCount(category.fileCount)} files · {formatBytes(category.totalBytes)}</small>
                        <small>{category.subSources.slice(0, 6).join(", ")}</small>
                      </span>
                    </label>
                  ))}
                </div>
              </>
            )}
            <div className="syncActions">
              {busyAction === "adb-pull" ? (
                <button className="pill" onClick={() => void cancelPullFromDevice()} type="button">Stop copying</button>
              ) : (
                <button className="primaryButton" disabled={Boolean(busyAction) || (!selectedSourcePath && selectedSource?.adapter !== "adb-generic")} onClick={handlePrimaryAction} type="button">
                  {busyAction ? "Working..." : primaryActionLabel()}
                </button>
              )}
              <button className="pill" onClick={() => setStep(1)} type="button">Back</button>
            </div>
          </>
        )}

        {/* STEP 3 — Preview */}
        {step === 3 && consolidationPlan && (
          <>
            <div className="summaryBox heroSummary">
              <strong>{formatCount(consolidationPlan.newFiles)} new · {formatCount(consolidationPlan.duplicateFiles)} already in your library</strong>
              <span>{formatBytes(consolidationPlan.newBytes)} of new data · {formatBytes(consolidationPlan.duplicateBytes)} already covered</span>
            </div>
            {progress && (
              <div className="summaryBox">
                <strong>{formatCount(progress.processedFiles)} / {formatCount(progress.totalFiles)} processed</strong>
                <span>{progress.currentPath}</span>
              </div>
            )}
            <div className="syncActions">
              <button className="primaryButton" disabled={Boolean(busyAction)} onClick={handleRunConsolidation} type="button">
                {busyAction === "import" ? "Importing..." : "Import and deduplicate"}
              </button>
              <button className="pill" onClick={() => { setConsolidationPlan(null); setStep(2); }} type="button">Back</button>
            </div>
          </>
        )}

        {/* STEP 4 — Done */}
        {step === 4 && (
          <>
            {progress && (
              <div className="summaryBox">
                <strong>{formatCount(progress.processedFiles)} / {formatCount(progress.totalFiles)} processed</strong>
                <span>{progress.currentPath}</span>
              </div>
            )}
            {consolidationResult && (
              <div className="summaryBox heroSummary">
                <strong>
                  {consolidationResult.copiedFiles > 0
                    ? `${formatCount(consolidationResult.copiedFiles)} new files added to your library`
                    : "Nothing new — everything was already in your library"}
                  {consolidationResult.occurrencesRecorded > consolidationResult.copiedFiles
                    ? ` · ${formatCount(consolidationResult.occurrencesRecorded - consolidationResult.copiedFiles)} already had a copy`
                    : ""}
                </strong>
                <span>Your originals were left untouched.</span>
                {consolidationResult.errors.length > 0 && (
                  <small>{consolidationResult.errors.length} warning(s): {consolidationResult.errors[0]}</small>
                )}
              </div>
            )}
            <div className="syncActions">
              {onNavigate && (
                <button className="primaryButton" onClick={() => onNavigate("gallery")} type="button">View your library →</button>
              )}
              <button className="pill" onClick={() => { setConsolidationPlan(null); setConsolidationResult(null); setStep(1); }} type="button">Import another source</button>
            </div>
          </>
        )}

        {/* Advanced — out of the guided flow, collapsed */}
        <div className="advancedPanel">
          <button className="pill" onClick={() => setShowAdvancedTools((current) => !current)} type="button">
            {showAdvancedTools ? "Hide advanced settings" : "Advanced settings"}
          </button>
          {showAdvancedTools && (
            <>
              <PathPickerField
                buttonLabel="Choose library folder"
                description="Where PhoneBridge stores deduplicated copies. Originals stay where they are. The default works for most people."
                label="PhoneBridge library (destination)"
                onChange={setDestinationPath}
                onChoose={chooseDestinationFolder}
                value={destinationPath}
              />
              <h2>Low-level tools</h2>
              <p className="mutedText">Use these only when you want a specific operation instead of the guided import.</p>
              <div className="syncActions">
                <button className="pill" onClick={handleIndexMultimedia} type="button">Index selected folder only</button>
                <button className="pill" onClick={handleRunSmartSwitchSync} type="button">Copy selected SmartSwitch categories</button>
                <button className="pill" onClick={handlePullFromDevice} type="button">Copy from phone to staging</button>
              </div>
              {summary && (
                <div className="summaryBox">
                  <strong>{formatCount(summary.scannedFiles)} files indexed</strong>
                  <span>{formatBytes(summary.totalBytes)} scanned from {summary.rootPath}</span>
                  <small>SQLite: {summary.databasePath}</small>
                </div>
              )}
              {syncResult && (
                <div className="summaryBox">
                  <strong>{formatCount(syncResult.copiedFiles)} copied · {formatCount(syncResult.skippedFiles)} skipped</strong>
                  <span>{formatBytes(syncResult.copiedBytes)} copied · {formatBytes(syncResult.skippedBytes)} skipped</span>
                  {syncResult.errors.length > 0 && <small>{syncResult.errors.length} warning(s): {syncResult.errors[0]}</small>}
                </div>
              )}
              {syncProgress && (
                <div className="summaryBox">
                  <strong>{formatCount(syncProgress.copiedFiles)} copied · {formatCount(syncProgress.skippedFiles)} skipped</strong>
                  <span>{syncProgress.currentPath}</span>
                </div>
              )}
              <h2>Safe-purge advisor</h2>
              <p className="mutedText">After importing, see which original backups are fully covered and safe to delete. PhoneBridge never deletes anything for you.</p>
              <div className="syncActions">
                <button className="pill" onClick={refreshCoverage} type="button">Refresh coverage</button>
              </div>
              {backupCoverage.length === 0 ? (
                <p>No consolidated backups recorded yet.</p>
              ) : (
                <div className="sourceList">
                  {backupCoverage.map((backup) => (
                    <div className="sourceRow" key={backup.backupId}>
                      <strong>{backup.label}</strong>
                      <span>{backup.coveragePercent.toFixed(1)}% covered · {backup.safeToDelete ? "safe to delete original" : "keep original"}</span>
                      <small>{formatCount(backup.coveredFiles)} / {formatCount(backup.totalFiles)} files · {formatBytes(backup.reclaimableBytes)} reclaimable</small>
                      <small>{backup.sourcePath}</small>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </section>
  );
}
