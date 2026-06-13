import { useEffect, useState } from "react";
import SectionHeader from "../components/SectionHeader";
import StatsCard from "../components/StatsCard";
import { getCategoryMetrics, scanBackupSources } from "../lib/api";
import { formatBytes, formatCount } from "../lib/format";
import type { BackupSource, CategoryMetric } from "../lib/types";

export default function Dashboard() {
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

  return (
    <section>
      <SectionHeader
        eyebrow="Overview"
        title="Recover, index, and explore Android backups locally."
        description="PhoneBridge starts with ADB and Samsung SmartSwitch support, then exposes a unified desktop view over media, messages, calls, contacts, notes, calendar data, and apps."
      />
      <div className="statsGrid">
        <StatsCard label="Local media" value={formatCount(fileCount)} detail="files indexed from user-selected folders" />
        <StatsCard label="Media volume" value={formatBytes(byteCount)} detail="photos, videos, music, and documents" />
        <StatsCard label="SmartSwitch" value={formatCount(sources.length)} detail="backup sources detected locally" />
        <StatsCard label="Privacy" value="100% local" detail="no cloud upload, no remote analytics by design" />
      </div>
      {error && <p className="errorText">{error}</p>}
      <div className="card roadmapCard">
        <h2>v0.1.0 scope</h2>
        <p>Build the core adapter model, SQLite index, ADB import, SmartSwitch parser, sync progress, and data viewers before the first open-source release.</p>
      </div>
    </section>
  );
}
