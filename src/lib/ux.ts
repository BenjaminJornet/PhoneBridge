export type StatusTone = "info" | "success" | "warning" | "error";

export function shortenPath(path: string): string {
  if (!path) {
    return "Not selected yet";
  }

  const normalized = path.replaceAll("\\", "/");
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length <= 3) {
    return path;
  }
  return `.../${parts.slice(-3).join("/")}`;
}

export function mapWhatsAppError(message: string): string {
  const lower = message.toLowerCase();
  if (lower.includes("crypt14 requires a 131-byte key payload")) {
    return "This crypt14 database needs the full WhatsApp key file. A 64-character hex key is not enough for crypt14.";
  }
  if (lower.includes("crypt15 requires a 32-byte root key")) {
    return "This crypt15 database needs a 32-byte root key, either as a key file or a 64-character hex value.";
  }
  if (lower.includes("unrecognized whatsapp key file format")) {
    return "The selected file does not look like a supported WhatsApp key. Choose the raw key file or paste a 64-character crypt15 key.";
  }
  if (lower.includes("key file or 64-character key is required")) {
    return "Choose a WhatsApp key file, or paste a 64-character crypt15 key, before decrypting.";
  }
  if (lower.includes("hex key must be 64")) {
    return "The pasted WhatsApp key must contain exactly 64 hexadecimal characters.";
  }
  return message;
}
