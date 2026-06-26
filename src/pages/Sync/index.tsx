import PathPickerField from "../../components/PathPickerField";
import SectionHeader from "../../components/SectionHeader";
import StatusCallout from "../../components/StatusCallout";
import { formatBytes, formatCount } from "../../lib/format";
import { useSyncState } from "./useSyncState";
import { useSyncProgress } from "./useSyncProgress";
import { useSyncHandlers } from "./useSyncHandlers";
import SourceSelectionStep from "./SourceSelectionStep";
import DataGatheringStep from "./DataGatheringStep";
import PreviewStep from "./PreviewStep";
import CompletionStep from "./CompletionStep";

interface SyncProps {
  onNavigate?: (page: "gallery") => void;
}

export default function Sync({ onNavigate }: SyncProps) {
  const state = useSyncState();
  useSyncProgress(state);
  const handlers = useSyncHandlers(state);

  const {
    sources,
    selectedSourceId,
    selectedSourcePath,
    categories,
    selectedCategories,
    destinationPath,
    summary,
    syncResult,
    adbPullResult,
    adbDiagnostic,
    mediaPreview,
    selectedPullKeys,
    consolidationPlan,
    consolidationResult,
    backupCoverage,
    progress,
    syncProgress,
    adbPullProgress,
    status,
    statusTone,
    showAdvancedTools,
    busyAction,
    step,
    setSelectedSourcePath,
    setSelectedCategories,
    setDestinationPath,
    setConsolidationPlan,
    setConsolidationResult,
    setStatus,
    setStatusTone,
    setShowAdvancedTools,
    setStep,
  } = state;

  const {
    handleIndexMultimedia,
    handleRunSmartSwitchSync,
    handlePreviewDeviceMedia,
    togglePullKey,
    handlePullFromDevice,
    selectSource,
    adapterLabel,
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
  } = handlers;

  const selectedSource = sources.find((source) => source.id === selectedSourceId || source.path === selectedSourcePath);
  const adbSources = sources.filter((source) => source.adapter === "adb-generic");
  const backupSources = sources.filter((source) => source.path);

  return (
    <section>
      <SectionHeader
        eyebrow="Guided import"
        title="Choose one source. Preview it. Then import safely."
        description="PhoneBridge keeps originals untouched. It copies data into your local library only after you review what will be added or deduplicated."
      />
      <div className="card" style={{ marginTop: "18px" }}>
        <div className="wizardSteps" aria-label="Import steps">
          <div className={step === 1 ? "step activeStep" : "step"}><span>1</span><p>Choose a source</p></div>
          <div className={step === 2 ? "step activeStep" : "step"}><span>2</span><p>Get the data</p></div>
          <div className={step === 3 ? "step activeStep" : "step"}><span>3</span><p>Preview</p></div>
          <div className={step === 4 ? "step activeStep" : "step"}><span>4</span><p>Done</p></div>
        </div>
        <StatusCallout title="Import status" message={status} tone={statusTone} />

        {/* STEP 1 — Choose a source */}
        {step === 1 && (
          <SourceSelectionStep
            sources={sources}
            selectedSourceId={selectedSourceId}
            selectedSource={selectedSource}
            adbSources={adbSources}
            backupSources={backupSources}
            busyAction={busyAction}
            adapterLabel={adapterLabel}
            selectSource={selectSource}
            refreshAdb={refreshAdb}
            chooseSmartSwitchFolder={chooseSmartSwitchFolder}
            chooseSourceFolder={chooseSourceFolder}
            setStatus={setStatus}
            setStatusTone={setStatusTone}
          />
        )}

        {/* STEP 2 — Get the data */}
        {step === 2 && (
          <DataGatheringStep
            selectedSource={selectedSource}
            selectedSourcePath={selectedSourcePath}
            adbDiagnostic={adbDiagnostic}
            mediaPreview={mediaPreview}
            selectedPullKeys={selectedPullKeys}
            adbPullResult={adbPullResult}
            adbPullProgress={adbPullProgress}
            categories={categories}
            selectedCategories={selectedCategories}
            busyAction={busyAction}
            adapterLabel={adapterLabel}
            setSelectedSourcePath={setSelectedSourcePath}
            setSelectedCategories={setSelectedCategories}
            setStep={setStep}
            chooseSourceFolder={chooseSourceFolder}
            handlePreviewDeviceMedia={handlePreviewDeviceMedia}
            togglePullKey={togglePullKey}
            toggleCategory={toggleCategory}
            selectAllCategories={selectAllCategories}
            handlePrimaryAction={handlePrimaryAction}
            primaryActionLabel={primaryActionLabel}
          />
        )}

        {/* STEP 3 — Preview */}
        {step === 3 && consolidationPlan && (
          <PreviewStep
            consolidationPlan={consolidationPlan}
            progress={progress}
            busyAction={busyAction}
            handleRunConsolidation={handleRunConsolidation}
            setConsolidationPlan={setConsolidationPlan}
            setStep={setStep}
          />
        )}

        {/* STEP 4 — Done */}
        {step === 4 && (
          <CompletionStep
            progress={progress}
            consolidationResult={consolidationResult}
            onNavigate={onNavigate}
            setConsolidationPlan={setConsolidationPlan}
            setConsolidationResult={setConsolidationResult}
            setStep={setStep}
          />
        )}

        {/* Advanced — out of the guided flow, collapsed */}
        <div className="advancedPanel">
          <button className="pill" onClick={() => setShowAdvancedTools((current) => !current)} type="button">
            {showAdvancedTools ? "Hide advanced settings" : "Advanced settings"}
          </button>
          {showAdvancedTools && (
            <>
              <PathPickerField
                buttonLabel="Choose library folder"
                description="Where PhoneBridge stores deduplicated copies. Originals stay where they are. The default works for most people."
                label="PhoneBridge library (destination)"
                onChange={setDestinationPath}
                onChoose={chooseDestinationFolder}
                value={destinationPath}
              />
              <h2>Low-level tools</h2>
              <p className="mutedText">Use these only when you want a specific operation instead of the guided import.</p>
              <div className="syncActions">
                <button className="pill" onClick={handleIndexMultimedia} type="button">Index selected folder only</button>
                <button className="pill" onClick={handleRunSmartSwitchSync} type="button">Copy selected SmartSwitch categories</button>
                <button className="pill" onClick={handlePullFromDevice} type="button">Copy from phone to staging</button>
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
              <h2>Safe-purge advisor</h2>
              <p className="mutedText">After importing, see which original backups are fully covered and safe to delete. PhoneBridge never deletes anything for you.</p>
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
            </>
          )}
        </div>
      </div>
    </section>
  );
}
