import { invoke } from "@tauri-apps/api/core";
import type {
  BackupSource,
  BackupCoverage,
  AdbDiagnostic,
  AdbMediaFolderPreview,
  AdbPullResult,
  AdapterDefinition,
  CategoryMetric,
  ConsolidationConfig,
  ConsolidationPlan,
  ConsolidationResult,
  IndexedFile,
  IndexSummary,
  SmartSwitchArchiveInventory,
  SmartSwitchCategory,
  SmartSwitchItemMetric,
  SmartSwitchSyncConfig,
  SmartSwitchSyncResult,
  StructuredRecord,
  WhatsAppDecryptConfig,
  WhatsAppDecryptResult,
  WhatsAppPullResult,
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

export function getAdapterRegistry(): Promise<AdapterDefinition[]> {
  return invokeIfAvailable("get_adapter_registry", []);
}

export function getCategoryMetrics(): Promise<CategoryMetric[]> {
  return invokeIfAvailable("get_category_metrics", []);
}

export function detectAdbDevices(): Promise<BackupSource[]> {
  return invokeIfAvailable("detect_adb_devices", []);
}

export function diagnoseAdb(): Promise<AdbDiagnostic> {
  return invokeIfAvailable("diagnose_adb", {
    adbFound: false,
    devices: [],
    message: "ADB diagnostics are available in the desktop app.",
    nextAction: "Open the packaged PhoneBridge app to check connected phones.",
  });
}

export function previewDeviceMedia(sourceId: string): Promise<AdbMediaFolderPreview[]> {
  return invokeIfAvailable("preview_device_media", [], { sourceId });
}

export function pullFromDevice(
  sourceId: string,
  destinationPath: string,
  selectedKeys?: string[],
): Promise<AdbPullResult> {
  return invokeIfAvailable("pull_from_device", {
    sourcePath: destinationPath,
    pulledPaths: 0,
    skippedPaths: 0,
    pulledFiles: 0,
    skippedFiles: 0,
    permissionDeniedFiles: 0,
    totalFiles: 0,
    errors: [],
  }, { sourceId, destinationPath, selectedKeys });
}

export function pullWhatsAppDatabase(sourceId: string, destinationDir: string): Promise<WhatsAppPullResult> {
  return invokeIfAvailable("pull_whatsapp_database", {
    localPath: "",
    remotePath: "",
    format: "",
  }, { sourceId, destinationDir });
}

export function decryptWhatsAppDatabase(config: WhatsAppDecryptConfig): Promise<WhatsAppDecryptResult> {
  return invokeIfAvailable("decrypt_whatsapp_database", {
    outputPath: config.outputPath,
    messageCount: 0,
    chatCount: 0,
    records: [],
  }, { config });
}

export function indexMultimedia(sourcePath: string): Promise<IndexSummary> {
  return invokeIfAvailable("index_multimedia", {
    databasePath: "",
    rootPath: sourcePath,
    scannedFiles: 0,
    indexedFiles: 0,
    totalBytes: 0,
  }, { sourcePath });
}

export function listIndexedFiles(category?: string, limit = 120, offset = 0): Promise<IndexedFile[]> {
  return invokeIfAvailable("list_indexed_files", [], { category, limit, offset });
}

export function getSmartSwitchItemMetrics(): Promise<SmartSwitchItemMetric[]> {
  return invokeIfAvailable("get_smartswitch_item_metrics", []);
}

export function getSmartSwitchArchiveInventory(): Promise<SmartSwitchArchiveInventory[]> {
  return invokeIfAvailable("get_smartswitch_archive_inventory", []);
}

export function getStructuredRecords(): Promise<StructuredRecord[]> {
  return invokeIfAvailable("get_structured_records", []);
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

export function planConsolidation(config: ConsolidationConfig): Promise<ConsolidationPlan> {
  return invokeIfAvailable("plan_consolidation", {
    sourcePath: config.sourcePath,
    destinationPath: config.destinationPath,
    deviceId: config.deviceId,
    deviceLabel: config.deviceLabel,
    totalFiles: 0,
    totalBytes: 0,
    newFiles: 0,
    duplicateFiles: 0,
    newBytes: 0,
    duplicateBytes: 0,
  }, { config });
}

export function runConsolidation(config: ConsolidationConfig): Promise<ConsolidationResult> {
  return invokeIfAvailable("run_consolidation", {
    runId: "",
    backupId: "",
    plan: {
      sourcePath: config.sourcePath,
      destinationPath: config.destinationPath,
      totalFiles: 0,
      totalBytes: 0,
      newFiles: 0,
      duplicateFiles: 0,
      newBytes: 0,
      duplicateBytes: 0,
    },
    copiedFiles: 0,
    duplicateFiles: 0,
    copiedBytes: 0,
    occurrencesRecorded: 0,
    errors: [],
  }, { config });
}

export async function openFile(path: string): Promise<void> {
  return invokeIfAvailable("open_file", undefined, { path });
}

export async function revealInFinder(path: string): Promise<void> {
  return invokeIfAvailable("reveal_in_finder", undefined, { path });
}

export function listBackupCoverage(): Promise<BackupCoverage[]> {
  return invokeIfAvailable("list_backup_coverage", []);
}
