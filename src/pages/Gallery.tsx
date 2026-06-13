import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import SectionHeader from "../components/SectionHeader";
import { listIndexedFiles } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { IndexedFile } from "../lib/types";

const filters = [
  { label: "All", value: undefined },
  { label: "Photos", value: "photo" },
  { label: "Videos", value: "video" },
  { label: "Music", value: "music" },
  { label: "Documents", value: "documents" },
] as const;

const previewablePhotoExtensions = new Set(["avif", "gif", "jpeg", "jpg", "png", "webp"]);

export default function Gallery() {
  const [activeCategory, setActiveCategory] = useState<string | undefined>();
  const [files, setFiles] = useState<IndexedFile[]>([]);
  const [status, setStatus] = useState("Loading indexed files...");

  useEffect(() => {
    let cancelled = false;
    setStatus("Loading indexed files...");

    listIndexedFiles(activeCategory, 120)
      .then((nextFiles) => {
        if (!cancelled) {
          setFiles(nextFiles);
          setStatus(nextFiles.length === 0 ? "No indexed files yet. Run Sync > Index local multimedia first." : "Ready");
        }
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setStatus(cause instanceof Error ? cause.message : String(cause));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [activeCategory]);

  return (
    <section>
      <SectionHeader
        eyebrow="Media"
        title="A fast local gallery for large Android archives."
        description="The gallery will use lazy thumbnails, date/source filters, and direct filesystem playback without loading large videos into memory."
      />
      <div className="filterRow">
        {filters.map((filter) => (
          <button
            className={filter.value === activeCategory ? "pill activePill" : "pill"}
            key={filter.label}
            onClick={() => setActiveCategory(filter.value)}
            type="button"
          >
            {filter.label}
          </button>
        ))}
      </div>
      <p className="mutedText">{status}</p>
      <div className="mediaGrid" aria-label="Indexed media grid">
        {files.map((file) => (
          <article className="mediaTile" key={file.id} title={file.absolutePath}>
            {file.category === "photo" && file.extension && previewablePhotoExtensions.has(file.extension) && (
              <img alt={file.relativePath} loading="lazy" src={convertFileSrc(file.absolutePath)} />
            )}
            <strong>{file.extension?.toUpperCase() ?? file.category}</strong>
            <span>{file.relativePath}</span>
            <small>{file.source} · {formatBytes(file.sizeBytes)}</small>
          </article>
        ))}
      </div>
    </section>
  );
}
