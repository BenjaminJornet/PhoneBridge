import { formatBytes, formatCount } from "../../lib/format";
import type { ConsolidationPlan } from "../../lib/types";
import type { ConsolidationProgress } from "./useSyncState";

interface PreviewStepProps {
  consolidationPlan: ConsolidationPlan;
  progress: ConsolidationProgress | null;
  busyAction: string | null;
  handleRunConsolidation: () => void | Promise<void>;
  setConsolidationPlan: (plan: ConsolidationPlan | null) => void;
  setStep: (step: 1 | 2 | 3 | 4) => void;
}

export default function PreviewStep({
  consolidationPlan,
  progress,
  busyAction,
  handleRunConsolidation,
  setConsolidationPlan,
  setStep,
}: PreviewStepProps) {
  return (
    <>
      <div className="summaryBox heroSummary">
        <strong>{formatCount(consolidationPlan.newFiles)} new · {formatCount(consolidationPlan.duplicateFiles)} already in your library</strong>
        <span>{formatBytes(consolidationPlan.newBytes)} of new data · {formatBytes(consolidationPlan.duplicateBytes)} already covered</span>
        {consolidationPlan.alreadyOnComputer > 0 && (
          <small>{formatCount(consolidationPlan.alreadyOnComputer)} of the new files already exist in another folder you indexed — they'll still be copied in so your library stays self-contained.</small>
        )}
      </div>
      {progress && (
        <div className="summaryBox">
          <strong>{formatCount(progress.processedFiles)} / {formatCount(progress.totalFiles)} processed</strong>
          <span>{progress.currentPath}</span>
        </div>
      )}
      <div className="syncActions">
        <button className="primaryButton" disabled={Boolean(busyAction)} onClick={handleRunConsolidation} type="button">
          {busyAction === "import" ? "Importing..." : "Import and deduplicate"}
        </button>
        <button className="pill" onClick={() => { setConsolidationPlan(null); setStep(2); }} type="button">Back</button>
      </div>
    </>
  );
}
