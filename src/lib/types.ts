export type DataCategory =
  | "photos"
  | "videos"
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

export interface BackupSource {
  id: string;
  adapter: "adb-generic" | "samsung-smartswitch";
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
