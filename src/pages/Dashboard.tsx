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
      <article className="card quickStartCard heroCard">
        <span>{hasLibrary ? "Your library" : "Get started"}</span>
        <h2>{hasLibrary ? "Continue with your library" : "Bring in your first source"}</h2>
        <p>
          {hasLibrary
            ? "Your local library already has data. Open it to browse, or import another backup — PhoneBridge deduplicates against what you already have."
            : sources.length > 0
              ? `PhoneBridge detected ${formatCount(adbSources.length)} phone(s) and ${formatCount(backupSources.length)} backup(s). The guided import walks you through it, one step at a time.`
              : "Start the guided import. You pick a source first, then preview exactly what will be added before anything is copied."}
        </p>
        <div className="syncActions compactActions">
          <button className="primaryButton" onClick={() => onNavigate(hasLibrary ? "gallery" : "sync")} type="button">
            {hasLibrary ? "Open my library" : "Start guided import"}
          </button>
          {hasLibrary && (
            <button className="pill" onClick={() => onNavigate("sync")} type="button">Import more</button>
          )}
        </div>
      </article>
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
