import { open } from "@tauri-apps/plugin-dialog";
import {
  diagnoseAdb,
  indexMultimedia,
  listBackupCoverage,
  planConsolidation,
  previewDeviceMedia,
  pullFromDevice,
  runSmartSwitchSync,
  runConsolidation,
  scanBackupSources,
  scanSmartSwitchCategories,
} from "../../lib/api";
import { formatBytes } from "../../lib/format";
import type { BackupSource } from "../../lib/types";
import type { SyncState } from "./useSyncState";

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

export function useSyncHandlers(state: SyncState) {
  const {
    sources,
    selectedSourceId,
    selectedSourcePath,
    selectedCategories,
    categories,
    destinationPath,
    mediaPreview,
    selectedPullKeys,
    consolidationPlan,
    adapterRegistry,
    setSources,
    setSelectedSourceId,
    setSelectedSourcePath,
    setCategories,
    setSelectedCategories,
    setDestinationPath,
    setSummary,
    setSyncResult,
    setSyncProgress,
    setAdbPullResult,
    setAdbPullProgress,
    setAdbDiagnostic,
    setMediaPreview,
    setSelectedPullKeys,
    setConsolidationPlan,
    setConsolidationResult,
    setBackupCoverage,
    setProgress,
    setStatus,
    setStatusTone,
    setBusyAction,
    setStep,
  } = state;

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

  function toggleCategory(category: string) {
    setSelectedCategories((current) =>
      current.includes(category) ? current.filter((item) => item !== category) : [...current, category],
    );
  }

  function selectAllCategories() {
    setSelectedCategories(categories.map((category) => category.name));
  }

  async function handlePrimaryAction() {
    const selectedSource = sources.find((source) => source.id === selectedSourceId || source.path === selectedSourcePath);
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
    const selectedSource = sources.find((source) => source.id === selectedSourceId || source.path === selectedSourcePath);
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

  return {
    handleIndexMultimedia,
    handleRunSmartSwitchSync,
    handlePreviewDeviceMedia,
    togglePullKey,
    handlePullFromDevice,
    selectSource,
    adapterLabel,
    handlePlanConsolidation,
    handleRunConsolidation,
    refreshCoverage,
    refreshAdb,
    chooseDestinationFolder,
    chooseSourceFolder,
    chooseSmartSwitchFolder,
    toggleCategory,
    selectAllCategories,
    handlePrimaryAction,
    primaryActionLabel,
  };
}

export type SyncHandlers = ReturnType<typeof useSyncHandlers>;
