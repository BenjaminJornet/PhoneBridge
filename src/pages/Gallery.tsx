import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import EmptyState from "../components/EmptyState";
import Lightbox from "../components/Lightbox";
import SectionHeader from "../components/SectionHeader";
import { listIndexedFiles } from "../lib/api";
import { formatBytes, formatCategoryLabel } from "../lib/format";
import type { IndexedFile } from "../lib/types";

const filters = [
  { label: "All", value: undefined },
  { label: "Photos", value: "photo" },
  { label: "Videos", value: "video" },
  { label: "Music", value: "music" },
  { label: "Documents", value: "documents" },
] as const;

const previewablePhotoExtensions = new Set(["avif", "gif", "jpeg", "jpg", "png", "webp"]);

interface GalleryProps {
  onImport: () => void;
}

export default function Gallery({ onImport }: GalleryProps) {
  const pageSize = 120;
  const [activeCategory, setActiveCategory] = useState<string | undefined>();
  const [files, setFiles] = useState<IndexedFile[]>([]);
  const [offset, setOffset] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [status, setStatus] = useState("Loading indexed files...");
  const [activeFile, setActiveFile] = useState<IndexedFile | null>(null);

  useEffect(() => {
    let cancelled = false;
    setOffset(0);
    setStatus("Loading indexed files...");

    listIndexedFiles(activeCategory, pageSize, 0)
      .then((nextFiles) => {
        if (!cancelled) {
          setFiles(nextFiles);
          setOffset(nextFiles.length);
          setHasMore(nextFiles.length === pageSize);
          setStatus(nextFiles.length === 0 ? "No files in this view yet." : "Ready");
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

  async function loadMore() {
    setLoadingMore(true);
    try {
      const nextFiles = await listIndexedFiles(activeCategory, pageSize, offset);
      setFiles((current) => [...current, ...nextFiles]);
      setOffset((current) => current + nextFiles.length);
      setHasMore(nextFiles.length === pageSize);
      setStatus("Ready");
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoadingMore(false);
    }
  }

  return (
    <section>
      <SectionHeader
        eyebrow="Media"
        title="Your recovered media library."
        description="Browse files that PhoneBridge has already imported or indexed locally. Nothing is uploaded anywhere."
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
      {files.length === 0 ? (
        <EmptyState
          title="Import something first."
          description="Choose an Android phone, SmartSwitch backup, or folder. PhoneBridge will preview the import before copying anything."
          actionLabel="Start guided import"
          onAction={onImport}
        />
      ) : (
        <div className="mediaGrid" aria-label="Indexed media grid">
          {files.map((file) => {
            // Prefer native path for web-displayable formats; fall back to generated thumbnail.
            const imgSrc = file.category === "photo"
              ? ((file.extension && previewablePhotoExtensions.has(file.extension))
                ? file.absolutePath
                : (file.thumbnailPath ?? null))
              : null;
            return (
              <article className="mediaTile" key={file.id} title={file.absolutePath} onClick={() => setActiveFile(file)} role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter") setActiveFile(file); }}>
                {imgSrc && (
                  <img alt={file.relativePath} loading="lazy" src={convertFileSrc(imgSrc)} />
                )}
                <strong>{file.extension?.toUpperCase() ?? formatCategoryLabel(file.category)}</strong>
                <span>{file.relativePath}</span>
                <small>{file.source} · {formatBytes(file.sizeBytes)}</small>
              </article>
            );
          })}
        </div>
      )}
      {files.length > 0 && hasMore && (
        <div className="syncActions">
          <button className="pill" disabled={loadingMore} onClick={() => void loadMore()} type="button">
            {loadingMore ? "Loading..." : "Load more"}
          </button>
        </div>
      )}
      {activeFile && <Lightbox file={activeFile} onClose={() => setActiveFile(null)} />}
    </section>
  );
}
