import { useState } from "react";
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
} from "../../lib/types";
import type { StatusTone } from "../../lib/ux";

export type ConsolidationProgress = { processedFiles: number; totalFiles: number; currentPath: string };
export type SyncProgressPayload = { copiedFiles: number; skippedFiles: number; currentPath: string };
export type AdbPullProgressPayload = {
  pulledPaths: number;
  skippedPaths: number;
  pulledFiles: number;
  skippedFiles: number;
  permissionDeniedFiles: number;
  totalFiles: number;
  currentPath: string;
};

export function useSyncState() {
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
  const [progress, setProgress] = useState<ConsolidationProgress | null>(null);
  const [syncProgress, setSyncProgress] = useState<SyncProgressPayload | null>(null);
  const [adbPullProgress, setAdbPullProgress] = useState<AdbPullProgressPayload | null>(null);
  const [status, setStatus] = useState("Ready");
  const [statusTone, setStatusTone] = useState<StatusTone>("info");
  const [showAdvancedTools, setShowAdvancedTools] = useState(false);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [step, setStep] = useState<1 | 2 | 3 | 4>(1);

  return {
    sources,
    setSources,
    adapterRegistry,
    setAdapterRegistry,
    selectedSourceId,
    setSelectedSourceId,
    selectedSourcePath,
    setSelectedSourcePath,
    categories,
    setCategories,
    selectedCategories,
    setSelectedCategories,
    destinationPath,
    setDestinationPath,
    summary,
    setSummary,
    syncResult,
    setSyncResult,
    adbPullResult,
    setAdbPullResult,
    adbDiagnostic,
    setAdbDiagnostic,
    mediaPreview,
    setMediaPreview,
    selectedPullKeys,
    setSelectedPullKeys,
    consolidationPlan,
    setConsolidationPlan,
    consolidationResult,
    setConsolidationResult,
    backupCoverage,
    setBackupCoverage,
    progress,
    setProgress,
    syncProgress,
    setSyncProgress,
    adbPullProgress,
    setAdbPullProgress,
    status,
    setStatus,
    statusTone,
    setStatusTone,
    showAdvancedTools,
    setShowAdvancedTools,
    busyAction,
    setBusyAction,
    step,
    setStep,
  };
}

export type SyncState = ReturnType<typeof useSyncState>;
