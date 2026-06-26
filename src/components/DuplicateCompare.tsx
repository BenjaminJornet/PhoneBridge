import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { openFile, revealInFinder } from "../lib/api";
import { formatBytes, formatCategoryLabel, formatDate } from "../lib/format";
import type { DuplicateGroup, IndexedFile } from "../lib/types";

const webDisplayableExtensions = new Set(["avif", "gif", "jpeg", "jpg", "png", "webp"]);

interface DuplicateCompareProps {
  group: DuplicateGroup;
  /** "identical" = exact SHA-256 duplicates, "similar" = perceptual look-alikes. */
  kind: "identical" | "similar";
  onClose: () => void;
  onTrashFile: (file: IndexedFile) => Promise<void>;
}

export default function DuplicateCompare({ group, kind, onClose, onTrashFile }: DuplicateCompareProps) {
  const isSimilar = kind === "similar";
  const panelRef = useRef<HTMLDivElement>(null);
  const [confirmingId, setConfirmingId] = useState<number | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);

  useEffect(() => {
    panelRef.current?.focus();
  }, []);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  async function handleTrash(file: IndexedFile) {
    if (confirmingId !== file.id) {
      setConfirmingId(file.id);
      return;
    }
    setBusyId(file.id);
    try {
      await onTrashFile(file);
    } finally {
      setBusyId(null);
      setConfirmingId(null);
    }
  }

  const remaining = group.files.length;

  return (
    <div className="lightboxOverlay" onClick={onClose} role="presentation">
      <div
        className="lightboxPanel comparePanel"
        ref={panelRef}
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          // Panel-level fallback: the document-capture listener above should
          // already catch Escape, but a focused button inside the panel can
          // swallow the key event before it reaches the document in the WebView.
          if (event.key === "Escape") {
            onClose();
          }
        }}
        role="dialog"
        aria-modal="true"
        aria-label="Compare duplicate copies"
      >
        <div className="compareHeader">
          <div>
            <strong>
              {isSimilar
                ? `${remaining} look-alike ${remaining === 1 ? "photo" : "photos"}`
                : `${remaining} identical ${remaining === 1 ? "copy" : "copies"}`}
            </strong>
            <small className="mutedText">
              {isSimilar
                ? `These photos look alike — matched by perceptual hash, not byte-identical. Keep the best, send the rest to the Trash.`
                : `Same content (${formatBytes(group.sizeBytes)}) · verified by SHA-256. Keep one, send the rest to the Trash.`}
            </small>
          </div>
          <button className="pill" onClick={onClose} type="button">Close</button>
        </div>
        <div className="compareGrid">
          {group.files.map((file) => {
            const nativeOk = Boolean(file.extension && webDisplayableExtensions.has(file.extension.toLowerCase()));
            const canDisplay = nativeOk || Boolean(file.thumbnailPath);
            const displaySrc = nativeOk ? file.absolutePath : (file.thumbnailPath ?? file.absolutePath);
            const confirming = confirmingId === file.id;
            return (
              <article className="compareCard" key={file.id}>
                <div className="compareStage">
                  {canDisplay ? (
                    <img alt={file.relativePath} src={convertFileSrc(displaySrc)} />
                  ) : (
                    <div className="comparePlaceholder">
                      <strong>{file.extension?.toUpperCase() ?? formatCategoryLabel(file.category)}</strong>
                      <span>Open in Preview to view full size.</span>
                    </div>
                  )}
                </div>
                <div className="compareMeta">
                  <strong title={file.relativePath}>{file.relativePath}</strong>
                  <small>{file.source} · {formatBytes(file.sizeBytes)} · {formatDate(file.modifiedUnix)}</small>
                  <code>{file.absolutePath}</code>
                </div>
                <div className="compareActions">
                  <button className="pill" onClick={() => void openFile(file.absolutePath)} type="button">Open</button>
                  <button className="pill" onClick={() => void revealInFinder(file.absolutePath)} type="button">Reveal</button>
                  <button
                    className={confirming ? "pill dangerPill" : "pill"}
                    disabled={busyId === file.id}
                    onClick={() => void handleTrash(file)}
                    type="button"
                  >
                    {busyId === file.id ? "Moving..." : confirming ? "Confirm Trash" : "Move to Trash"}
                  </button>
                </div>
              </article>
            );
          })}
        </div>
      </div>
    </div>
  );
}
