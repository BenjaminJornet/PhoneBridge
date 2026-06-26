import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect } from "react";
import { openFile, revealInFinder } from "../lib/api";
import { formatBytes, formatCategoryLabel } from "../lib/format";
import type { IndexedFile } from "../lib/types";

const webDisplayableExtensions = new Set(["avif", "gif", "jpeg", "jpg", "png", "webp"]);

interface LightboxProps {
  file: IndexedFile;
  onClose: () => void;
}

export default function Lightbox({ file, onClose }: LightboxProps) {
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const canDisplay = Boolean(file.extension && webDisplayableExtensions.has(file.extension.toLowerCase()));

  return (
    <div className="lightboxOverlay" onClick={onClose} role="presentation">
      <div className="lightboxPanel" onClick={(event) => event.stopPropagation()} role="dialog" aria-modal="true">
        <div className="lightboxStage">
          {canDisplay ? (
            <img alt={file.relativePath} src={convertFileSrc(file.absolutePath)} />
          ) : (
            <div className="lightboxPlaceholder">
              <strong>{file.extension?.toUpperCase() ?? formatCategoryLabel(file.category)}</strong>
              <p>This format can&apos;t be previewed in the app. Open it in Preview to view it full size.</p>
            </div>
          )}
        </div>
        <div className="lightboxMeta">
          <strong>{file.relativePath}</strong>
          <small>{file.source} · {formatBytes(file.sizeBytes)}</small>
          <code>{file.absolutePath}</code>
          <div className="syncActions compactActions">
            <button className="primaryButton" onClick={() => void openFile(file.absolutePath)} type="button">Open in Preview</button>
            <button className="pill" onClick={() => void revealInFinder(file.absolutePath)} type="button">Reveal in Finder</button>
            <button className="pill" onClick={onClose} type="button">Close</button>
          </div>
        </div>
      </div>
    </div>
  );
}
