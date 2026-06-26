import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";
import { openFile, revealInFinder } from "../lib/api";
import { useEscapeToClose } from "../lib/useEscapeToClose";
import { formatBytes, formatCategoryLabel } from "../lib/format";
import type { IndexedFile } from "../lib/types";

const webDisplayableExtensions = new Set(["avif", "gif", "jpeg", "jpg", "png", "webp"]);

interface LightboxProps {
  file: IndexedFile;
  onClose: () => void;
}

export default function Lightbox({ file, onClose }: LightboxProps) {
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    panelRef.current?.focus();
  }, []);

  useEscapeToClose(onClose);

  const canDisplayNatively = Boolean(file.extension && webDisplayableExtensions.has(file.extension.toLowerCase()));
  const canDisplay = canDisplayNatively || Boolean(file.thumbnailPath);
  const displaySrc = canDisplayNatively ? file.absolutePath : (file.thumbnailPath ?? file.absolutePath);

  return (
    <div className="lightboxOverlay" onClick={onClose} role="presentation">
      <div className="lightboxPanel" ref={panelRef} tabIndex={-1} onClick={(event) => event.stopPropagation()} role="dialog" aria-modal="true">
        <div className="lightboxStage">
          {canDisplay ? (
            <>
              <img alt={file.relativePath} src={convertFileSrc(displaySrc)} />
              {!canDisplayNatively && file.thumbnailPath && (
                <p className="mutedText">Thumbnail preview — open in Preview for the full-quality original.</p>
              )}
            </>
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
