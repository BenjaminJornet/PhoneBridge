import { useEffect, useState } from "react";
import SectionHeader from "../components/SectionHeader";
import { getCategoryMetrics, getSmartSwitchArchiveInventory, getSmartSwitchItemMetrics } from "../lib/api";
import { formatBytes, formatCount } from "../lib/format";
import type { CategoryMetric, SmartSwitchArchiveInventory, SmartSwitchItemMetric } from "../lib/types";

const categories = [
  { label: "Contacts", status: "Inventory available in Tauri; detailed payload is encrypted" },
  { label: "Messages", status: "SmartSwitch parser planned" },
  { label: "Call log", status: "Inventory available in Tauri; detailed payload is binary/proprietary" },
  { label: "Calendar", status: "SmartSwitch parser planned" },
  { label: "Samsung Notes", status: "SmartSwitch parser planned" },
  { label: "Apps/APKs", status: "SmartSwitch parser planned" },
  { label: "Browser data", status: "SmartSwitch parser planned" },
];

export default function DataExplorer() {
  const [metrics, setMetrics] = useState<CategoryMetric[]>([]);
  const [smartSwitchMetrics, setSmartSwitchMetrics] = useState<SmartSwitchItemMetric[]>([]);
  const [archiveInventory, setArchiveInventory] = useState<SmartSwitchArchiveInventory[]>([]);

  useEffect(() => {
    let cancelled = false;
    Promise.all([getCategoryMetrics(), getSmartSwitchItemMetrics(), getSmartSwitchArchiveInventory()])
      .then(([nextMetrics, nextSmartSwitchMetrics, nextArchiveInventory]) => {
        if (!cancelled) {
          setMetrics(nextMetrics);
          setSmartSwitchMetrics(nextSmartSwitchMetrics);
          setArchiveInventory(nextArchiveInventory);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setSmartSwitchMetrics([]);
          setArchiveInventory([]);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <section>
      <SectionHeader
        eyebrow="Structured data"
        title="Beyond media: parse what SmartSwitch already contains."
        description="SmartSwitch backups can include contacts, messages, call logs, calendar data, notes, app metadata, and permissions."
      />
      <div className="dataGrid">
        {metrics.map((metric) => (
          <article className="card dataCard" key={metric.category}>
            <h2>{metric.category}</h2>
            <strong>{formatCount(metric.count)} files</strong>
            <p>{formatBytes(metric.bytes)} indexed locally.</p>
          </article>
        ))}
        {smartSwitchMetrics.slice(0, 12).map((metric) => (
          <article className="card dataCard" key={`${metric.backupId}-${metric.itemType}`}>
            <h2>{metric.itemType}</h2>
            <strong>{formatCount(metric.contentCount)} items</strong>
            <p>{formatBytes(metric.sizeBytes)} in {metric.backupLabel}.</p>
          </article>
        ))}
        {archiveInventory.map((item) => (
          <article className="card dataCard" key={`${item.backupId}-${item.itemType}-inventory`}>
            <h2>{item.itemType} archive</h2>
            <strong>{formatCount(item.entryCount)} archive entries</strong>
            <p>Status: {item.parseStatus}</p>
            <small>{formatCount(item.encryptedEntries)} encrypted · {formatCount(item.imageEntries)} images · {formatCount(item.blobEntries)} blobs</small>
          </article>
        ))}
        {categories.map((category) => (
          <article className="card dataCard" key={category.label}>
            <h2>{category.label}</h2>
            <p>{category.status}</p>
          </article>
        ))}
      </div>
    </section>
  );
}
