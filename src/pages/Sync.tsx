import { useEffect, useState } from "react";
import SectionHeader from "../components/SectionHeader";
import {
  detectAdbDevices,
  indexMultimedia,
  runSmartSwitchSync,
  scanBackupSources,
  scanSmartSwitchCategories,
} from "../lib/api";
import { formatBytes, formatCount } from "../lib/format";
import type { BackupSource, IndexSummary, SmartSwitchCategory, SmartSwitchSyncResult } from "../lib/types";

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
  const [destinationPath, setDestinationPath] = useState("~/Samsung/Multimedia");
  const [summary, setSummary] = useState<IndexSummary | null>(null);
  const [syncResult, setSyncResult] = useState<SmartSwitchSyncResult | null>(null);
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
    setStatus("Indexing ~/Samsung/Multimedia...");
    try {
      const nextSummary = await indexMultimedia();
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
    try {
      const result = await runSmartSwitchSync({
        sourcePath: selectedSourcePath,
        destinationPath,
        categories: selectedCategories,
      });
      setSyncResult(result);
      setStatus("SmartSwitch sync complete");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
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
          <button className="primaryButton" onClick={handleRunSmartSwitchSync} type="button">
            Sync selected SmartSwitch categories
          </button>
          <button className="primaryButton" onClick={handleIndexMultimedia} type="button">
            Index local multimedia
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
                {source.path && <small>{source.path}</small>}
              </button>
            ))}
          </div>
        )}
        <h2>Destination</h2>
        <label className="pathField">
          <span>Aggregated target folder</span>
          <input value={destinationPath} onChange={(event) => setDestinationPath(event.target.value)} />
        </label>
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
      </div>
    </section>
  );
}
