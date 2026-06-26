import { formatCount } from "../../lib/format";
import type { ConsolidationPlan, ConsolidationResult } from "../../lib/types";
import type { ConsolidationProgress } from "./useSyncState";

interface CompletionStepProps {
  progress: ConsolidationProgress | null;
  consolidationResult: ConsolidationResult | null;
  onNavigate?: (page: "gallery") => void;
  setConsolidationPlan: (plan: ConsolidationPlan | null) => void;
  setConsolidationResult: (result: ConsolidationResult | null) => void;
  setStep: (step: 1 | 2 | 3 | 4) => void;
}

export default function CompletionStep({
  progress,
  consolidationResult,
  onNavigate,
  setConsolidationPlan,
  setConsolidationResult,
  setStep,
}: CompletionStepProps) {
  return (
    <>
      {progress && (
        <div className="summaryBox">
          <strong>{formatCount(progress.processedFiles)} / {formatCount(progress.totalFiles)} processed</strong>
          <span>{progress.currentPath}</span>
        </div>
      )}
      {consolidationResult && (
        <div className="summaryBox heroSummary">
          <strong>
            {consolidationResult.copiedFiles > 0
              ? `${formatCount(consolidationResult.copiedFiles)} new files added to your library`
              : "Nothing new — everything was already in your library"}
            {consolidationResult.occurrencesRecorded > consolidationResult.copiedFiles
              ? ` · ${formatCount(consolidationResult.occurrencesRecorded - consolidationResult.copiedFiles)} already had a copy`
              : ""}
          </strong>
          <span>Your originals were left untouched.</span>
          {consolidationResult.errors.length > 0 && (
            <small>{consolidationResult.errors.length} warning(s): {consolidationResult.errors[0]}</small>
          )}
        </div>
      )}
      <div className="syncActions">
        {onNavigate && (
          <button className="primaryButton" onClick={() => onNavigate("gallery")} type="button">View your library →</button>
        )}
        <button className="pill" onClick={() => { setConsolidationPlan(null); setConsolidationResult(null); setStep(1); }} type="button">Import another source</button>
      </div>
    </>
  );
}
