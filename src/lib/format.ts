export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }

  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exponent;

  return `${value.toFixed(exponent === 0 ? 0 : 1)} ${units[exponent]}`;
}

export function formatCount(value: number): string {
  return new Intl.NumberFormat("en-US").format(value);
}

/** Format a Unix timestamp (seconds) as a short human date, or "—" when absent. */
export function formatDate(unixSeconds?: number): string {
  if (!unixSeconds || !Number.isFinite(unixSeconds)) {
    return "—";
  }
  return new Date(unixSeconds * 1000).toLocaleDateString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

const CATEGORY_LABELS: Record<string, string> = {
  photo: "Photos",
  video: "Videos",
  music: "Music",
  documents: "Documents",
  other: "Other",
};

export function formatCategoryLabel(category: string): string {
  return CATEGORY_LABELS[category] ?? category;
}
