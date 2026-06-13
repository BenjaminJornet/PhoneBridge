import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import SectionHeader from "../components/SectionHeader";
import {
  detectAdbDevices,
  indexMultimedia,
  listBackupCoverage,
  planConsolidation,
  runSmartSwitchSync,
  runConsolidation,
  scanBackupSources,
  scanSmartSwitchCategories,
} from "../lib/api";
import { formatBytes, formatCount } from "../lib/format";
import type {
  BackupCoverage,
  BackupSource,
  ConsolidationPlan,
  ConsolidationResult,
  IndexSummary,
  SmartSwitchCategory,
  SmartSwitchSyncResult,
} from "../lib/types";

const syncSteps = [
  "Detect connected Android devices via ADB",
  "Scan Samsung SmartSwitch backups from local folders",
  "Plan category imports without overwriting existing files",
  "Index recovered data into SQLite",
  "Generate thumbnails and searchable metadata",
];

export default function Sync() {
  const [sources, setSources] = useState<BackupSource[]>([]);
  const [selectedSourcePath, setSelectedSourcePath] = useState("");
  const [categories, setCategories] = useState<SmartSwitchCategory[]>([]);
  const [selectedCategories, setSelectedCategories] = useState<string[]>([]);
  const [destinationPath, setDestinationPath] = useState("~/.phonebridge/library");
  const [summary, setSummary] = useState<IndexSummary | null>(null);
  const [syncResult, setSyncResult] = useState<SmartSwitchSyncResult | null>(null);
  const [consolidationPlan, setConsolidationPlan] = useState<ConsolidationPlan | null>(null);
  const [consolidationResult, setConsolidationResult] = useState<ConsolidationResult | null>(null);
  const [backupCoverage, setBackupCoverage] = useState<BackupCoverage[]>([]);
  const [progress, setProgress] = useState<{ processedFiles: number; totalFiles: number; currentPath: string } | null>(null);
  const [syncProgress, setSyncProgress] = useState<{ copiedFiles: number; skippedFiles: number; currentPath: string } | null>(null);
  const [status, setStatus] = useState("Ready");

  useEffect(() => {
    let cancelled = false;

    Promise.all([scanBackupSources(), detectAdbDevices()])
      .then(([backupSources, adbSources]) => {
        if (!cancelled) {
          const nextSources = [...backupSources, ...adbSources];
          setSources(nextSources);
          const firstSmartSwitch = nextSources.find((source) => source.adapter === "samsung-smartswitch" && source.path);
          if (firstSmartSwitch?.path) {
            setSelectedSourcePath(firstSmartSwitch.path);
          }
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
  }, []);

  useEffect(() => {
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
  }, [selectedSourcePath]);

  async function handleIndexMultimedia() {
    if (!selectedSourcePath) {
      setStatus("Choose a source folder before indexing.");
      return;
    }

    setStatus("Indexing selected folder...");
    try {
      const nextSummary = await indexMultimedia(selectedSourcePath);
      setSummary(nextSummary);
      setStatus("Index complete");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function handleRunSmartSwitchSync() {
    if (!selectedSourcePath || selectedCategories.length === 0) {
      setStatus("Select a SmartSwitch source and at least one category.");
      return;
    }

    setStatus("Synchronizing SmartSwitch backup without deleting or overwriting files...");
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
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
    }
  }

  function consolidationConfig() {
    return {
      sourcePath: selectedSourcePath,
      destinationPath,
      adapter: sources.find((source) => source.path === selectedSourcePath)?.adapter ?? "generic-folder",
      label: sources.find((source) => source.path === selectedSourcePath)?.label ?? "SmartSwitch backup",
      deviceId: sources.find((source) => source.path === selectedSourcePath)?.device?.id,
      deviceLabel: sources.find((source) => source.path === selectedSourcePath)?.label,
    };
  }

  async function handlePlanConsolidation() {
    if (!selectedSourcePath) {
      setStatus("Select a source before planning consolidation.");
      return;
    }

    setStatus("Planning content-deduplicated consolidation...");
    setConsolidationPlan(null);
    setConsolidationResult(null);
    try {
      const plan = await planConsolidation(consolidationConfig());
      setConsolidationPlan(plan);
      setStatus("Dry-run plan ready");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function handleRunConsolidation() {
    if (!selectedSourcePath) {
      setStatus("Select a source before consolidation.");
      return;
    }

    setStatus("Consolidating by content hash without deleting originals...");
    setConsolidationResult(null);
    setProgress(null);
    try {
      const result = await runConsolidation(consolidationConfig());
      setConsolidationResult(result);
      setConsolidationPlan(result.plan);
      setBackupCoverage(await listBackupCoverage());
      setProgress(null);
      setStatus("Consolidation complete");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function refreshCoverage() {
    try {
      setBackupCoverage(await listBackupCoverage());
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function chooseDestinationFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setDestinationPath(selected);
    }
  }

  async function chooseSourceFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setSelectedSourcePath(selected);
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

  return (
    <section>
      <SectionHeader
        eyebrow="Sync engine"
        title="One workflow for live devices and local backups."
        description="This screen will replace the shell scripts with resumable, observable imports and explicit category selection."
      />
      <div className="card listCard">
        <div className="syncActions">
          <button className="primaryButton" onClick={handlePlanConsolidation} type="button">
            Preview consolidation
          </button>
          <button className="primaryButton" onClick={handleRunConsolidation} type="button">
            Consolidate by content
          </button>
          <button className="primaryButton" onClick={handleRunSmartSwitchSync} type="button">
            Sync selected SmartSwitch categories
          </button>
          <button className="primaryButton" onClick={handleIndexMultimedia} type="button">
            Index selected folder
          </button>
          <span>{status}</span>
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
        {consolidationPlan && (
          <div className="summaryBox">
            <strong>
              {formatCount(consolidationPlan.newFiles)} new · {formatCount(consolidationPlan.duplicateFiles)} duplicates
            </strong>
            <span>
              {formatBytes(consolidationPlan.newBytes)} new · {formatBytes(consolidationPlan.duplicateBytes)} already covered
            </span>
            <small>Dry-run source: {consolidationPlan.sourcePath}</small>
          </div>
        )}
        {consolidationResult && (
          <div className="summaryBox">
            <strong>
              {formatCount(consolidationResult.copiedFiles)} stored · {formatCount(consolidationResult.occurrencesRecorded)} occurrences
            </strong>
            <span>Backup id: {consolidationResult.backupId}</span>
            <small>Run id: {consolidationResult.runId}</small>
            {consolidationResult.errors.length > 0 && (
              <small>{consolidationResult.errors.length} warning(s): {consolidationResult.errors[0]}</small>
            )}
          </div>
        )}
        {progress && (
          <div className="summaryBox">
            <strong>{formatCount(progress.processedFiles)} / {formatCount(progress.totalFiles)} processed</strong>
            <span>{progress.currentPath}</span>
          </div>
        )}
        <h2>Detected sources</h2>
        {sources.length === 0 ? (
          <p>No backup source or authorized ADB device detected yet.</p>
        ) : (
          <div className="sourceList">
            {sources.map((source) => (
              <button
                className={source.path === selectedSourcePath ? "sourceRow selectedSource" : "sourceRow"}
                disabled={!source.path || source.adapter !== "samsung-smartswitch"}
                key={source.id}
                onClick={() => source.path && setSelectedSourcePath(source.path)}
                type="button"
              >
                <strong>{source.label}</strong>
                <span>{source.adapter}</span>
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
        )}
        <h2>Destination</h2>
        <div className="syncActions">
          <button className="pill" onClick={chooseSourceFolder} type="button">Choose source folder</button>
          <button className="pill" onClick={chooseDestinationFolder} type="button">Choose destination folder</button>
        </div>
        <label className="pathField">
          <span>Aggregated target folder</span>
          <input value={destinationPath} onChange={(event) => setDestinationPath(event.target.value)} />
        </label>
        <small>Content-addressed library target: {destinationPath}</small>
        <h2>SmartSwitch categories</h2>
        <div className="syncActions">
          <button className="pill" onClick={selectAllCategories} type="button">Select all</button>
          <button className="pill" onClick={() => setSelectedCategories([])} type="button">Clear</button>
        </div>
        {categories.length === 0 ? (
          <p>No media category found for the selected SmartSwitch source.</p>
        ) : (
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
        )}
        <h2>Engine steps</h2>
        {syncSteps.map((step, index) => (
          <div className="step" key={step}>
            <span>{index + 1}</span>
            <p>{step}</p>
          </div>
        ))}
        <h2>Safe-purge advisor</h2>
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
      </div>
    </section>
  );
}
