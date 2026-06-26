export type DataCategory =
  | "photo"
  | "video"
  | "music"
  | "documents"
  | "messages"
  | "contacts"
  | "calls"
  | "calendar"
  | "notes"
  | "apps";

export interface DeviceSummary {
  id: string;
  model: string;
  manufacturer: string;
  androidVersion?: string;
  connection: "adb" | "backup";
}

export type BackupAdapter = "adb-generic" | "generic-folder" | "google-takeout" | "samsung-smartswitch";

export interface AdapterDefinition {
  id: BackupAdapter;
  label: string;
  description: string;
}

export interface BackupSource {
  id: string;
  adapter: BackupAdapter;
  label: string;
  path?: string;
  device?: DeviceSummary;
  createdAt?: string;
}

export interface CategoryMetric {
  category: DataCategory;
  count: number;
  bytes: number;
}

export interface IndexSummary {
  databasePath: string;
  rootPath: string;
  scannedFiles: number;
  indexedFiles: number;
  totalBytes: number;
}

export interface IndexedFile {
  id: number;
  absolutePath: string;
  relativePath: string;
  category: string;
  source: string;
  extension?: string;
  sizeBytes: number;
  modifiedUnix?: number;
  thumbnailPath?: string;
}

export interface SmartSwitchItemMetric {
  backupId: string;
  backupLabel: string;
  itemType: string;
  viewCount: number;
  contentCount: number;
  sizeBytes: number;
}

export interface SmartSwitchArchiveInventory {
  backupId: string;
  backupLabel: string;
  itemType: string;
  archivePath: string;
  entryCount: number;
  encryptedEntries: number;
  imageEntries: number;
  blobEntries: number;
  parseStatus: string;
}

export interface SyncPlan {
  sourceId: string;
  categories: DataCategory[];
  destination: string;
}

export interface SmartSwitchCategory {
  name: string;
  sourcePath: string;
  fileCount: number;
  totalBytes: number;
  subSources: string[];
}

export interface SmartSwitchSyncConfig {
  sourcePath: string;
  destinationPath: string;
  categories: string[];
}

export interface SmartSwitchSyncResult {
  copiedFiles: number;
  skippedFiles: number;
  copiedBytes: number;
  skippedBytes: number;
  errors: string[];
}

export interface AdbMediaFolderPreview {
  key: string;
  label: string;
  remotePath: string;
  fileCount: number;
  totalBytes: number;
  available: boolean;
}

export interface AdbPullResult {
  sourcePath: string;
  pulledPaths: number;
  skippedPaths: number;
  pulledFiles: number;
  skippedFiles: number;
  permissionDeniedFiles: number;
  totalFiles: number;
  errors: string[];
  cancelled: boolean;
}

export interface DuplicateGroup {
  hash: string;
  sizeBytes: number;
  reclaimableBytes: number;
  files: IndexedFile[];
}

export interface DuplicateScanResult {
  groups: DuplicateGroup[];
  totalGroups: number;
  reclaimableBytes: number;
  scannedCandidates: number;
}

export interface TrashResult {
  trashed: number;
  removedFromIndex: number;
  errors: string[];
}

export interface AdbDiagnosticDevice {
  sourceId: string;
  label: string;
  status: string;
  model?: string;
  manufacturer?: string;
  androidVersion?: string;
  redactedId: string;
}

export interface AdbDiagnostic {
  adbFound: boolean;
  adbPath?: string;
  devices: AdbDiagnosticDevice[];
  message: string;
  nextAction: string;
}

export interface WhatsAppPullResult {
  localPath: string;
  remotePath: string;
  format: string;
}

export interface WhatsAppDecryptConfig {
  encryptedDbPath: string;
  keyPath?: string;
  keyHex?: string;
  outputPath: string;
}

export interface WhatsAppDecryptResult {
  outputPath: string;
  messageCount: number;
  chatCount: number;
  records: StructuredRecord[];
}

export interface ConsolidationConfig {
  sourcePath: string;
  destinationPath: string;
  adapter: string;
  label: string;
  deviceId?: string;
  deviceLabel?: string;
}

export interface StructuredRecord {
  id: string;
  kind: string;
  title: string;
  subtitle?: string;
  sourcePath: string;
  parseStatus: string;
}

export interface ConsolidationPlan {
  sourcePath: string;
  destinationPath: string;
  totalFiles: number;
  totalBytes: number;
  newFiles: number;
  duplicateFiles: number;
  newBytes: number;
  duplicateBytes: number;
  /** New-to-the-library files that already exist in a folder indexed elsewhere on
   *  the computer. Informational — they are still copied into the library. */
  alreadyOnComputer: number;
}

export interface ConsolidationResult {
  runId: string;
  backupId: string;
  plan: ConsolidationPlan;
  copiedFiles: number;
  duplicateFiles: number;
  copiedBytes: number;
  occurrencesRecorded: number;
  errors: string[];
}

export interface BackupCoverage {
  backupId: string;
  label: string;
  sourcePath: string;
  totalFiles: number;
  coveredFiles: number;
  totalBytes: number;
  coveredBytes: number;
  coveragePercent: number;
  reclaimableBytes: number;
  safeToDelete: boolean;
}
