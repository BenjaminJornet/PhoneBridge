import { invoke } from "@tauri-apps/api/core";
import type {
  BackupSource,
  CategoryMetric,
  IndexedFile,
  IndexSummary,
  SmartSwitchArchiveInventory,
  SmartSwitchCategory,
  SmartSwitchItemMetric,
  SmartSwitchSyncConfig,
  SmartSwitchSyncResult,
} from "./types";

function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function invokeIfAvailable<T>(command: string, fallback: T, args?: Record<string, unknown>): Promise<T> {
  if (!hasTauriRuntime()) {
    return fallback;
  }

  return invoke(command, args);
}

export function scanBackupSources(): Promise<BackupSource[]> {
  return invokeIfAvailable("scan_backup_sources", []);
}

export function getCategoryMetrics(): Promise<CategoryMetric[]> {
  return invokeIfAvailable("get_category_metrics", []);
}

export function detectAdbDevices(): Promise<BackupSource[]> {
  return invokeIfAvailable("detect_adb_devices", []);
}

export function indexMultimedia(): Promise<IndexSummary> {
  return invokeIfAvailable("index_multimedia", {
    databasePath: "",
    rootPath: "",
    scannedFiles: 0,
    indexedFiles: 0,
    totalBytes: 0,
  });
}

export function listIndexedFiles(category?: string, limit = 120): Promise<IndexedFile[]> {
  return invokeIfAvailable("list_indexed_files", [], { category, limit });
}

export function getSmartSwitchItemMetrics(): Promise<SmartSwitchItemMetric[]> {
  return invokeIfAvailable("get_smartswitch_item_metrics", []);
}

export function getSmartSwitchArchiveInventory(): Promise<SmartSwitchArchiveInventory[]> {
  return invokeIfAvailable("get_smartswitch_archive_inventory", []);
}

export function scanSmartSwitchCategories(sourcePath: string): Promise<SmartSwitchCategory[]> {
  return invokeIfAvailable("scan_smartswitch_categories", [], { sourcePath });
}

export function runSmartSwitchSync(config: SmartSwitchSyncConfig): Promise<SmartSwitchSyncResult> {
  return invokeIfAvailable("run_smartswitch_sync", {
    copiedFiles: 0,
    skippedFiles: 0,
    copiedBytes: 0,
    skippedBytes: 0,
    errors: [],
  }, { config });
}
