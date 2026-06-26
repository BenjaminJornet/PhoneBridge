import PathPickerField from "../../components/PathPickerField";
import { formatBytes, formatCount } from "../../lib/format";
import { cancelPullFromDevice } from "../../lib/api";
import type {
  AdbDiagnostic,
  AdbMediaFolderPreview,
  AdbPullResult,
  BackupSource,
  SmartSwitchCategory,
} from "../../lib/types";
import type { AdbPullProgressPayload } from "./useSyncState";

interface DataGatheringStepProps {
  selectedSource: BackupSource | undefined;
  selectedSourcePath: string;
  adbDiagnostic: AdbDiagnostic | null;
  mediaPreview: AdbMediaFolderPreview[] | null;
  selectedPullKeys: string[];
  adbPullResult: AdbPullResult | null;
  adbPullProgress: AdbPullProgressPayload | null;
  categories: SmartSwitchCategory[];
  selectedCategories: string[];
  busyAction: string | null;
  adapterLabel: (adapterId: string) => string;
  setSelectedSourcePath: (path: string) => void;
  setSelectedCategories: (categories: string[]) => void;
  setStep: (step: 1 | 2 | 3 | 4) => void;
  chooseSourceFolder: () => void | Promise<void>;
  handlePreviewDeviceMedia: () => void | Promise<void>;
  togglePullKey: (key: string) => void;
  toggleCategory: (category: string) => void;
  selectAllCategories: () => void;
  handlePrimaryAction: () => void | Promise<void>;
  primaryActionLabel: () => string;
}

export default function DataGatheringStep({
  selectedSource,
  selectedSourcePath,
  adbDiagnostic,
  mediaPreview,
  selectedPullKeys,
  adbPullResult,
  adbPullProgress,
  categories,
  selectedCategories,
  busyAction,
  adapterLabel,
  setSelectedSourcePath,
  setSelectedCategories,
  setStep,
  chooseSourceFolder,
  handlePreviewDeviceMedia,
  togglePullKey,
  toggleCategory,
  selectAllCategories,
  handlePrimaryAction,
  primaryActionLabel,
}: DataGatheringStepProps) {
  return (
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
  );
}
