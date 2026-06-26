import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import PathPickerField from "../components/PathPickerField";
import SectionHeader from "../components/SectionHeader";
import StatusCallout from "../components/StatusCallout";
import { decryptWhatsAppDatabase, detectAdbDevices, getCategoryMetrics, getSmartSwitchArchiveInventory, getSmartSwitchItemMetrics, getStructuredRecords, pullWhatsAppDatabase } from "../lib/api";
import { formatBytes, formatCategoryLabel, formatCount } from "../lib/format";
import type { CategoryMetric, SmartSwitchArchiveInventory, SmartSwitchItemMetric, StructuredRecord, WhatsAppDecryptResult } from "../lib/types";
import { mapWhatsAppError } from "../lib/ux";
import type { StatusTone } from "../lib/ux";

const categories = [
  { label: "Contacts", status: "SmartSwitch encrypted contacts parse into local structured records" },
  { label: "Messages", status: "WhatsApp crypt14/15 decrypt works locally with user-provided key material" },
  { label: "Call log", status: "SmartSwitch encrypted call logs parse into local structured records" },
  { label: "Calendar", status: "ICS/VCS calendar exports parse into structured records" },
  { label: "Samsung Notes", status: "Text notes and typed .sdocx content parse locally" },
  { label: "Apps/APKs", status: "APK folders are inventoried without executing packages" },
  { label: "Browser data", status: "Readable SBROWSER JSON/HTML exports parse into records" },
];

interface DataExplorerProps {
  onNavigate?: (page: "gallery") => void;
}

export default function DataExplorer({ onNavigate }: DataExplorerProps) {
  const [metrics, setMetrics] = useState<CategoryMetric[]>([]);
  const [smartSwitchMetrics, setSmartSwitchMetrics] = useState<SmartSwitchItemMetric[]>([]);
  const [archiveInventory, setArchiveInventory] = useState<SmartSwitchArchiveInventory[]>([]);
  const [structuredRecords, setStructuredRecords] = useState<StructuredRecord[]>([]);
  const [whatsAppDbPath, setWhatsAppDbPath] = useState("");
  const [whatsAppKeyPath, setWhatsAppKeyPath] = useState("");
  const [whatsAppKeyHex, setWhatsAppKeyHex] = useState("");
  const [whatsAppOutputPath, setWhatsAppOutputPath] = useState("~/.phonebridge/whatsapp/msgstore.db");
  const [whatsAppResult, setWhatsAppResult] = useState<WhatsAppDecryptResult | null>(null);
  const [whatsAppStatus, setWhatsAppStatus] = useState("Choose your encrypted WhatsApp database. PhoneBridge will decrypt locally only after you provide your own key.");
  const [whatsAppTone, setWhatsAppTone] = useState<StatusTone>("info");
  const [showWhatsAppAdvanced, setShowWhatsAppAdvanced] = useState(false);
  const [pullingWhatsApp, setPullingWhatsApp] = useState(false);

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

  async function pullWhatsAppFromPhone() {
    setPullingWhatsApp(true);
    setWhatsAppStatus("Looking for a connected Android phone...");
    setWhatsAppTone("info");
    try {
      const devices = await detectAdbDevices();
      const phone = devices.find((device) => device.adapter === "adb-generic");
      if (!phone) {
        setWhatsAppStatus("No authorized Android phone detected. Connect it by USB and accept the debugging prompt, then try again.");
        setWhatsAppTone("warning");
        return;
      }
      setWhatsAppStatus("Copying the encrypted WhatsApp database from your phone...");
      const result = await pullWhatsAppDatabase(phone.id, "~/.phonebridge/whatsapp");
      setWhatsAppDbPath(result.localPath);
      setWhatsAppStatus(`WhatsApp database copied (${result.format}). Now provide your key below to decrypt it — PhoneBridge cannot extract the key from the phone.`);
      setWhatsAppTone("success");
    } catch (cause) {
      setWhatsAppStatus(mapWhatsAppError(cause instanceof Error ? cause.message : String(cause)));
      setWhatsAppTone("error");
    } finally {
      setPullingWhatsApp(false);
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
      setWhatsAppTone("warning");
      return;
    }
    setWhatsAppStatus("Decrypting locally. PhoneBridge never extracts keys automatically.");
    setWhatsAppTone("info");
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
      setWhatsAppTone("success");
    } catch (cause) {
      setWhatsAppStatus(mapWhatsAppError(cause instanceof Error ? cause.message : String(cause)));
      setWhatsAppTone("error");
    }
  }

  const whatsAppReady = Boolean(whatsAppDbPath && (whatsAppKeyPath || whatsAppKeyHex.trim()));

  return (
    <section>
      <SectionHeader
        eyebrow="Structured data"
        title="Recover messages, contacts, notes, apps, and other backup data."
        description="Most data appears automatically after an import. WhatsApp is separate because encrypted databases require key material that PhoneBridge will never extract by itself."
      />
      <div className="dataGrid">
        <article className="card dataCard">
          <span className="eyebrowText">Guided helper</span>
          <h2>WhatsApp messages</h2>
          <p>Copy the encrypted database straight from your connected phone, then provide your own key to decrypt it locally. Everything stays on this computer.</p>
          <div className="wizardSteps compactWizard" aria-label="WhatsApp decrypt steps">
            <div className={whatsAppDbPath ? "step activeStep" : "step"}><span>1</span><p>Get the database</p></div>
            <div className={whatsAppKeyPath || whatsAppKeyHex ? "step activeStep" : "step"}><span>2</span><p>Provide key</p></div>
            <div className={whatsAppReady ? "step activeStep" : "step"}><span>3</span><p>Decrypt locally</p></div>
          </div>
          <div className="syncActions compactActions">
            <button className="primaryButton" disabled={pullingWhatsApp} onClick={pullWhatsAppFromPhone} type="button">
              {pullingWhatsApp ? "Copying from phone..." : "Get WhatsApp database from phone"}
            </button>
          </div>
          <PathPickerField
            buttonLabel="Or choose a file"
            description="Auto-filled when you copy from the phone. Or pick msgstore.db.crypt14 / msgstore.db.crypt15 manually."
            label="Encrypted WhatsApp database"
            onChange={setWhatsAppDbPath}
            onChoose={chooseWhatsAppDb}
            value={whatsAppDbPath}
          />
          <PathPickerField
            buttonLabel="Choose key file"
            description="For crypt14, this is usually the full key file. For crypt15, you can also paste a 64-character key below."
            label="WhatsApp key file"
            onChange={setWhatsAppKeyPath}
            onChoose={chooseWhatsAppKey}
            value={whatsAppKeyPath}
          />
          <label className="pathField">
            <span>Or paste a crypt15 key</span>
            <input value={whatsAppKeyHex} onChange={(event) => setWhatsAppKeyHex(event.target.value)} placeholder="64 hexadecimal characters, optional" />
          </label>
          <p className="mutedText">The decryption key lives in WhatsApp&apos;s private storage and can&apos;t be copied without rooting the phone — PhoneBridge never does that. You provide the key yourself (from a backup export or a rooted pull).</p>
          <button className="pill" onClick={() => setShowWhatsAppAdvanced((current) => !current)} type="button">
            {showWhatsAppAdvanced ? "Hide advanced output" : "Show advanced output"}
          </button>
          {showWhatsAppAdvanced && (
            <label className="pathField">
              <span>Output SQLite DB</span>
              <input value={whatsAppOutputPath} onChange={(event) => setWhatsAppOutputPath(event.target.value)} />
            </label>
          )}
          <div className="syncActions">
            <button className="primaryButton" disabled={!whatsAppReady} onClick={decryptWhatsApp} type="button">Decrypt locally</button>
          </div>
          <StatusCallout title="WhatsApp helper" message={whatsAppStatus} tone={whatsAppTone} />
          {whatsAppResult && (
            <p>{formatCount(whatsAppResult.messageCount)} messages · {formatCount(whatsAppResult.chatCount)} chats · {whatsAppResult.outputPath}</p>
          )}
        </article>
        {metrics.map((metric) => (
          <article className="card dataCard" key={metric.category}>
            <h2>{formatCategoryLabel(metric.category)}</h2>
            <strong>{formatCount(metric.count)} files</strong>
            <p>{formatBytes(metric.bytes)} indexed locally.</p>
            {onNavigate && (
              <button className="pill" onClick={() => onNavigate("gallery")} type="button">Browse in Library →</button>
            )}
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
