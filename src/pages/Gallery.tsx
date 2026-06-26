import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import DuplicateCompare from "../components/DuplicateCompare";
import EmptyState from "../components/EmptyState";
import Lightbox from "../components/Lightbox";
import SectionHeader from "../components/SectionHeader";
import StatusCallout from "../components/StatusCallout";
import { findDuplicateFiles, findSimilarPhotos, listIndexedFiles, moveFilesToTrash } from "../lib/api";
import { formatBytes, formatCategoryLabel } from "../lib/format";
import type { DuplicateGroup, IndexedFile } from "../lib/types";

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

type ScanMode = "duplicates" | "similar";

// Big "look-alike" sets are usually coarse-hash false positives (text-heavy
// screenshots), so cap how many tiles we render per group to keep the DOM light.
const GROUP_TILE_CAP = 24;

/** Pick which copy to KEEP by default and return the rest's ids (pre-checked for Trash).
 *  - duplicates: identical content (SHA-256) → safe to pre-select the extras. Keep the
 *    copy not named like a SmartSwitch "DUPLICATE_…" file, else files[0].
 *  - similar: a perceptual match is fuzzy, so we DON'T pre-select anything — the user
 *    reviews each set and chooses what to trash. Returns []. */
function autoSelectForTrash(group: DuplicateGroup, mode: ScanMode): number[] {
  if (mode === "similar") {
    return [];
  }
  const isDuplicateNamed = (file: IndexedFile) =>
    (file.relativePath.split("/").pop() ?? "").toLowerCase().startsWith("duplicate");
  const keep = group.files.find((file) => !isDuplicateNamed(file)) ?? group.files[0];
  return group.files.filter((file) => file.id !== keep.id).map((file) => file.id);
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

  // Duplicate-finder state. scanMode null = normal browsing.
  const [scanMode, setScanMode] = useState<ScanMode | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState<{ done: number; total: number } | null>(null);
  const [groups, setGroups] = useState<DuplicateGroup[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [confirmingHash, setConfirmingHash] = useState<string | null>(null);
  const [trashingHash, setTrashingHash] = useState<string | null>(null);
  const [compareGroup, setCompareGroup] = useState<DuplicateGroup | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);

  useEffect(() => {
    if (scanMode) {
      return;
    }
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
  }, [activeCategory, scanMode]);

  // Run (or re-run) the active scan whenever a scan mode is on. Exact-duplicate scans
  // are scoped to the active category; the perceptual "similar" scan is photos-only.
  useEffect(() => {
    if (!scanMode) {
      return;
    }
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    setScanning(true);
    setScanError(null);
    setScanProgress(null);
    setGroups([]);
    setSelected(new Set());
    setConfirmingHash(null);
    setCompareGroup(null);

    const progressEvent = scanMode === "similar" ? "similar-scan-progress" : "duplicate-scan-progress";
    listen<{ done: number; total: number }>(progressEvent, (event) => {
      if (!cancelled) {
        setScanProgress(event.payload);
      }
    }).then((nextUnlisten) => {
      if (cancelled) {
        nextUnlisten();
      } else {
        unlisten = nextUnlisten;
      }
    });

    const scan = scanMode === "similar" ? findSimilarPhotos() : findDuplicateFiles(activeCategory);
    scan
      .then((result) => {
        if (cancelled) {
          return;
        }
        setGroups(result.groups);
        const preselected = new Set<number>();
        for (const group of result.groups) {
          for (const id of autoSelectForTrash(group, scanMode)) {
            preselected.add(id);
          }
        }
        setSelected(preselected);
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setScanError(cause instanceof Error ? cause.message : String(cause));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setScanning(false);
        }
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [activeCategory, scanMode]);

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

  function toggleSelected(id: number) {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
    setConfirmingHash(null);
  }

  function removeFilesFromState(ids: Set<number>) {
    setGroups((current) =>
      current
        .map((group) => ({ ...group, files: group.files.filter((file) => !ids.has(file.id)) }))
        .filter((group) => group.files.length >= 2),
    );
    setSelected((current) => {
      const next = new Set(current);
      for (const id of ids) {
        next.delete(id);
      }
      return next;
    });
  }

  async function trashSelectedInGroup(group: DuplicateGroup) {
    const ids = group.files.filter((file) => selected.has(file.id)).map((file) => file.id);
    if (ids.length === 0 || ids.length >= group.files.length) {
      return;
    }
    if (confirmingHash !== group.hash) {
      setConfirmingHash(group.hash);
      return;
    }
    setTrashingHash(group.hash);
    try {
      const paths = group.files.filter((file) => ids.includes(file.id)).map((file) => file.absolutePath);
      const result = await moveFilesToTrash(paths);
      const trashedIds = new Set(
        group.files.filter((file) => paths.includes(file.absolutePath)).map((file) => file.id),
      );
      // Only drop the rows the backend actually removed from the index.
      if (result.removedFromIndex > 0 || result.trashed > 0) {
        removeFilesFromState(trashedIds);
      }
      if (result.errors.length > 0) {
        setScanError(result.errors.join(" · "));
      }
    } catch (cause) {
      setScanError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setTrashingHash(null);
      setConfirmingHash(null);
    }
  }

  async function trashSingleFromCompare(file: IndexedFile) {
    const result = await moveFilesToTrash([file.absolutePath]);
    if (result.trashed > 0 || result.removedFromIndex > 0) {
      removeFilesFromState(new Set([file.id]));
      setCompareGroup((current) =>
        current ? { ...current, files: current.files.filter((item) => item.id !== file.id) } : current,
      );
    }
    if (result.errors.length > 0) {
      setScanError(result.errors.join(" · "));
    }
  }

  const totalReclaimable = groups.reduce((sum, group) => sum + group.reclaimableBytes, 0);
  const isSimilar = scanMode === "similar";

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
        <button
          className={scanMode === "duplicates" ? "pill activePill duplicateToggle" : "pill duplicateToggle"}
          onClick={() => setScanMode((current) => (current === "duplicates" ? null : "duplicates"))}
          type="button"
        >
          {scanMode === "duplicates" ? "← Back to all files" : "Find duplicates"}
        </button>
        <button
          className={scanMode === "similar" ? "pill activePill" : "pill"}
          onClick={() => setScanMode((current) => (current === "similar" ? null : "similar"))}
          type="button"
        >
          {scanMode === "similar" ? "← Back to all files" : "Find similar photos"}
        </button>
      </div>

      {scanMode ? (
        <>
          {scanning ? (
            <StatusCallout
              tone="info"
              title={isSimilar ? "Scanning for look-alike photos..." : "Scanning for duplicates..."}
              message={isSimilar
                ? (scanProgress
                  ? `Comparing how ${scanProgress.done} of ${scanProgress.total} photos look...`
                  : "Comparing how your photos look (perceptual hash), not just their exact bytes.")
                : (scanProgress
                  ? `Hashing ${scanProgress.done} of ${scanProgress.total} same-size files...`
                  : "Looking for files that share the exact same size, then comparing their content.")}
            />
          ) : scanError ? (
            <StatusCallout tone="error" title="Something went wrong" message={scanError} />
          ) : groups.length === 0 ? (
            <EmptyState
              title={isSimilar ? "No look-alike photos found." : "No duplicates found."}
              description={isSimilar
                ? "No two photos in your library look close enough to be near-duplicates. Nothing to clean up here."
                : "Every file in this view has unique content (verified by SHA-256). Nothing to clean up here."}
              actionLabel="Back to all files"
              onAction={() => setScanMode(null)}
            />
          ) : (
            <>
              <StatusCallout
                tone="success"
                title={isSimilar
                  ? `${groups.length} look-alike ${groups.length === 1 ? "set" : "sets"} · ${formatBytes(totalReclaimable)} reclaimable`
                  : `${groups.length} duplicate ${groups.length === 1 ? "set" : "sets"} · ${formatBytes(totalReclaimable)} reclaimable`}
                message={isSimilar
                  ? "Each set holds photos that look alike (matched by perceptual hash, not byte-identical) — so nothing is pre-selected. Click a tile to compare side by side, then check the ones you want to remove. Large sets can include false matches."
                  : "Each set holds identical content (verified by SHA-256). One copy is kept by default; the extras are pre-selected for the Trash. Click a tile to compare copies side by side."}
              />
              {scanError && (
                <StatusCallout tone="error" title="Some files could not be moved" message={scanError} />
              )}
              {groups.map((group) => {
                const selectedInGroup = group.files.filter((file) => selected.has(file.id)).length;
                const wouldRemoveAll = selectedInGroup >= group.files.length;
                const confirming = confirmingHash === group.hash;
                return (
                  <div className="duplicateGroup card" key={group.hash}>
                    <div className="duplicateGroupHeader">
                      <div>
                        <strong>
                          {isSimilar
                            ? `${group.files.length} look-alikes · up to ${formatBytes(group.sizeBytes)}`
                            : `${group.files.length} copies · ${formatBytes(group.sizeBytes)} each`}
                        </strong>
                        <small className="mutedText">{formatBytes(group.reclaimableBytes)} reclaimable if you keep {isSimilar ? "the best" : "one"}</small>
                      </div>
                      <button
                        className={confirming ? "primaryButton dangerButton" : "primaryButton"}
                        disabled={selectedInGroup === 0 || wouldRemoveAll || trashingHash === group.hash}
                        onClick={() => void trashSelectedInGroup(group)}
                        type="button"
                      >
                        {trashingHash === group.hash
                          ? "Moving..."
                          : wouldRemoveAll
                            ? "Keep at least one"
                            : confirming
                              ? `Confirm Trash (${selectedInGroup})`
                              : `Move ${selectedInGroup} to Trash`}
                      </button>
                    </div>
                    <div className="duplicateTiles">
                      {group.files.slice(0, GROUP_TILE_CAP).map((file) => {
                        const isSelected = selected.has(file.id);
                        const imgSrc = file.category === "photo"
                          ? ((file.extension && previewablePhotoExtensions.has(file.extension))
                            ? file.absolutePath
                            : (file.thumbnailPath ?? null))
                          : null;
                        return (
                          <article
                            className={isSelected ? "mediaTile duplicateTileSelected" : "mediaTile"}
                            key={file.id}
                            title={file.absolutePath}
                          >
                            <label className="duplicateCheck" onClick={(event) => event.stopPropagation()}>
                              <input
                                type="checkbox"
                                checked={isSelected}
                                onChange={() => toggleSelected(file.id)}
                              />
                              <span>{isSelected ? "Trash" : "Keep"}</span>
                            </label>
                            <div
                              className="duplicateTileBody"
                              onClick={() => setCompareGroup(group)}
                              role="button"
                              tabIndex={0}
                              onKeyDown={(event) => { if (event.key === "Enter") setCompareGroup(group); }}
                            >
                              {imgSrc && <img alt={file.relativePath} loading="lazy" src={convertFileSrc(imgSrc)} />}
                              <strong>{file.extension?.toUpperCase() ?? formatCategoryLabel(file.category)}</strong>
                              <span>{file.relativePath}</span>
                              <small>{file.source} · {formatBytes(file.sizeBytes)}</small>
                            </div>
                          </article>
                        );
                      })}
                      {group.files.length > GROUP_TILE_CAP && (
                        <p className="mutedText">
                          + {group.files.length - GROUP_TILE_CAP} more in this set. Large look-alike sets often include false matches — refine by comparing before removing anything.
                        </p>
                      )}
                    </div>
                  </div>
                );
              })}
            </>
          )}
        </>
      ) : (
        <>
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
        </>
      )}

      {activeFile && <Lightbox file={activeFile} onClose={() => setActiveFile(null)} />}
      {compareGroup && (
        <DuplicateCompare
          group={compareGroup}
          kind={isSimilar ? "similar" : "identical"}
          onClose={() => setCompareGroup(null)}
          onTrashFile={trashSingleFromCompare}
        />
      )}
    </section>
  );
}
