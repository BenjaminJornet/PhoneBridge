import { useEffect, useState } from "react";
import EmptyState from "../components/EmptyState";
import SectionHeader from "../components/SectionHeader";
import StatsCard from "../components/StatsCard";
import StatusCallout from "../components/StatusCallout";
import { getCategoryMetrics, scanBackupSources } from "../lib/api";
import { formatBytes, formatCount } from "../lib/format";
import type { BackupSource, CategoryMetric } from "../lib/types";

interface DashboardProps {
  onNavigate: (page: "dashboard" | "sync" | "gallery" | "data") => void;
}

export default function Dashboard({ onNavigate }: DashboardProps) {
  const [metrics, setMetrics] = useState<CategoryMetric[]>([]);
  const [sources, setSources] = useState<BackupSource[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    Promise.all([getCategoryMetrics(), scanBackupSources()])
      .then(([nextMetrics, nextSources]) => {
        if (!cancelled) {
          setMetrics(nextMetrics);
          setSources(nextSources);
        }
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const fileCount = metrics.reduce((sum, item) => sum + item.count, 0);
  const byteCount = metrics.reduce((sum, item) => sum + item.bytes, 0);
  const hasLibrary = fileCount > 0;
  const adbSources = sources.filter((source) => source.adapter === "adb-generic");
  const backupSources = sources.filter((source) => source.path);

  return (
    <section>
      <SectionHeader
        eyebrow="Start here"
        title="Bring your Android data into one private local library."
        description="Choose a phone, a backup, or a folder. PhoneBridge previews what it finds, imports only with your confirmation, and never deletes your originals."
      />
      <div className="statsGrid">
        <StatsCard label="Library" value={formatCount(fileCount)} detail="files already indexed locally" />
        <StatsCard label="Media volume" value={formatBytes(byteCount)} detail="photos, videos, music, and documents" />
        <StatsCard label="Detected sources" value={formatCount(sources.length)} detail="phones and backups found on this machine" />
        <StatsCard label="Privacy" value="100% local" detail="no cloud upload or telemetry" />
      </div>
      {error && <StatusCallout title="Detection issue" message={error} tone="warning" />}
      <div className="quickStartGrid">
        <article className="card quickStartCard">
          <span>Recommended</span>
          <h2>{hasLibrary ? "Continue exploring your library" : "Import your first source"}</h2>
          <p>
            {hasLibrary
              ? "Your local library already has data. Open it, or import another backup to deduplicate against what is already covered."
              : "Start with the guided import. You will choose a source first, then preview what PhoneBridge will add before anything is copied."}
          </p>
          <div className="syncActions compactActions">
            <button className="primaryButton" onClick={() => onNavigate(hasLibrary ? "gallery" : "sync")} type="button">
              {hasLibrary ? "Open library" : "Start guided import"}
            </button>
            <button className="pill" onClick={() => onNavigate("data")} type="button">Open data tools</button>
          </div>
        </article>
        <article className="card quickStartCard">
          <span>Detected now</span>
          <h2>{formatCount(adbSources.length)} phone(s) · {formatCount(backupSources.length)} backup(s)</h2>
          <p>
            {sources.length > 0
              ? "PhoneBridge found sources it can use. The guided import will explain what each source contains before importing."
              : "No phone or known backup was detected automatically. You can still choose any folder manually."}
          </p>
          <button className="pill" onClick={() => onNavigate("sync")} type="button">Choose a source</button>
        </article>
      </div>
      {!hasLibrary && (
        <EmptyState
          title="Nothing has been imported yet."
          description="The app is intentionally empty on first launch. Pick a source, preview the import, then let PhoneBridge build your local library."
          actionLabel="Import data"
          onAction={() => onNavigate("sync")}
        />
      )}
    </section>
  );
}
