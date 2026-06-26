import type { BackupSource } from "../../lib/types";

interface SourceSelectionStepProps {
  sources: BackupSource[];
  selectedSourceId: string;
  selectedSource: BackupSource | undefined;
  adbSources: BackupSource[];
  backupSources: BackupSource[];
  busyAction: string | null;
  adapterLabel: (adapterId: string) => string;
  selectSource: (source: BackupSource) => void;
  refreshAdb: () => void | Promise<void>;
  chooseSmartSwitchFolder: () => void | Promise<void>;
  chooseSourceFolder: () => void | Promise<void>;
  setStatus: (status: string) => void;
  setStatusTone: (tone: "info" | "success" | "warning" | "error") => void;
}

export default function SourceSelectionStep({
  sources,
  selectedSourceId,
  selectedSource,
  adbSources,
  backupSources,
  busyAction,
  adapterLabel,
  selectSource,
  refreshAdb,
  chooseSmartSwitchFolder,
  chooseSourceFolder,
  setStatus,
  setStatusTone,
}: SourceSelectionStepProps) {
  return (
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
  );
}
