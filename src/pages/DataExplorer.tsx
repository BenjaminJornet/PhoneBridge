import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import SectionHeader from "../components/SectionHeader";
import { decryptWhatsAppDatabase, getCategoryMetrics, getSmartSwitchArchiveInventory, getSmartSwitchItemMetrics, getStructuredRecords } from "../lib/api";
import { formatBytes, formatCount } from "../lib/format";
import type { CategoryMetric, SmartSwitchArchiveInventory, SmartSwitchItemMetric, StructuredRecord, WhatsAppDecryptResult } from "../lib/types";

const categories = [
  { label: "Contacts", status: "SmartSwitch encrypted contacts parse into local structured records" },
  { label: "Messages", status: "WhatsApp crypt14/15 decrypt works locally with user-provided key material" },
  { label: "Call log", status: "SmartSwitch encrypted call logs parse into local structured records" },
  { label: "Calendar", status: "ICS/VCS calendar exports parse into structured records" },
  { label: "Samsung Notes", status: "Text notes and typed .sdocx content parse locally" },
  { label: "Apps/APKs", status: "APK folders are inventoried without executing packages" },
  { label: "Browser data", status: "Readable SBROWSER JSON/HTML exports parse into records" },
];

export default function DataExplorer() {
  const [metrics, setMetrics] = useState<CategoryMetric[]>([]);
  const [smartSwitchMetrics, setSmartSwitchMetrics] = useState<SmartSwitchItemMetric[]>([]);
  const [archiveInventory, setArchiveInventory] = useState<SmartSwitchArchiveInventory[]>([]);
  const [structuredRecords, setStructuredRecords] = useState<StructuredRecord[]>([]);
  const [whatsAppDbPath, setWhatsAppDbPath] = useState("");
  const [whatsAppKeyPath, setWhatsAppKeyPath] = useState("");
  const [whatsAppKeyHex, setWhatsAppKeyHex] = useState("");
  const [whatsAppOutputPath, setWhatsAppOutputPath] = useState("~/.phonebridge/whatsapp/msgstore.db");
  const [whatsAppResult, setWhatsAppResult] = useState<WhatsAppDecryptResult | null>(null);
  const [whatsAppStatus, setWhatsAppStatus] = useState("Select an encrypted WhatsApp DB and your own key file or 64-character key.");

  useEffect(() => {
    let cancelled = false;
    Promise.all([getCategoryMetrics(), getSmartSwitchItemMetrics(), getSmartSwitchArchiveInventory(), getStructuredRecords()])
      .then(([nextMetrics, nextSmartSwitchMetrics, nextArchiveInventory, nextStructuredRecords]) => {
        if (!cancelled) {
          setMetrics(nextMetrics);
          setSmartSwitchMetrics(nextSmartSwitchMetrics);
          setArchiveInventory(nextArchiveInventory);
          setStructuredRecords(nextStructuredRecords);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setSmartSwitchMetrics([]);
          setArchiveInventory([]);
          setStructuredRecords([]);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  async function chooseWhatsAppDb() {
    const selected = await open({ multiple: false });
    if (typeof selected === "string") {
      setWhatsAppDbPath(selected);
    }
  }

  async function chooseWhatsAppKey() {
    const selected = await open({ multiple: false });
    if (typeof selected === "string") {
      setWhatsAppKeyPath(selected);
    }
  }

  async function decryptWhatsApp() {
    if (!whatsAppDbPath || (!whatsAppKeyPath && !whatsAppKeyHex.trim())) {
      setWhatsAppStatus("Choose the encrypted DB and provide a key file or 64-character key first.");
      return;
    }
    setWhatsAppStatus("Decrypting locally. PhoneBridge never extracts keys automatically.");
    setWhatsAppResult(null);
    try {
      const result = await decryptWhatsAppDatabase({
        encryptedDbPath: whatsAppDbPath,
        keyPath: whatsAppKeyPath || undefined,
        keyHex: whatsAppKeyHex.trim() || undefined,
        outputPath: whatsAppOutputPath,
      });
      setWhatsAppResult(result);
      setStructuredRecords((current) => [...result.records, ...current]);
      setWhatsAppStatus("WhatsApp database decrypted locally.");
    } catch (cause) {
      setWhatsAppStatus(cause instanceof Error ? cause.message : String(cause));
    }
  }

  return (
    <section>
      <SectionHeader
        eyebrow="Structured data"
        title="Beyond media: parse what SmartSwitch already contains."
        description="SmartSwitch backups can include contacts, messages, call logs, calendar data, notes, app metadata, and permissions."
      />
      <div className="dataGrid">
        <article className="card dataCard">
          <h2>WhatsApp messages</h2>
          <p>Local decrypt only. Provide your own key material; PhoneBridge never roots the phone or extracts keys automatically.</p>
          <div className="syncActions">
            <button className="pill" onClick={chooseWhatsAppDb} type="button">Choose encrypted DB</button>
            <button className="pill" onClick={chooseWhatsAppKey} type="button">Choose key file</button>
            <button className="primaryButton" onClick={decryptWhatsApp} type="button">Decrypt locally</button>
          </div>
          <label className="pathField">
            <span>Encrypted DB</span>
            <input value={whatsAppDbPath} onChange={(event) => setWhatsAppDbPath(event.target.value)} placeholder="msgstore.db.crypt14 or .crypt15" />
          </label>
          <label className="pathField">
            <span>Key file</span>
            <input value={whatsAppKeyPath} onChange={(event) => setWhatsAppKeyPath(event.target.value)} placeholder="key or encrypted_backup.key" />
          </label>
          <label className="pathField">
            <span>64-character key</span>
            <input value={whatsAppKeyHex} onChange={(event) => setWhatsAppKeyHex(event.target.value)} placeholder="optional crypt15 hex key" />
          </label>
          <label className="pathField">
            <span>Output SQLite DB</span>
            <input value={whatsAppOutputPath} onChange={(event) => setWhatsAppOutputPath(event.target.value)} />
          </label>
          <small>{whatsAppStatus}</small>
          {whatsAppResult && (
            <p>{formatCount(whatsAppResult.messageCount)} messages · {formatCount(whatsAppResult.chatCount)} chats · {whatsAppResult.outputPath}</p>
          )}
        </article>
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
        {structuredRecords.map((record) => (
          <article className="card dataCard" key={record.id}>
            <h2>{record.kind}</h2>
            <strong>{record.title}</strong>
            {record.subtitle && <p>{record.subtitle}</p>}
            <small>Status: {record.parseStatus}</small>
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
