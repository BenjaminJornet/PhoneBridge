import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import DataExplorer from "./DataExplorer";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../lib/api", () => ({
  decryptWhatsAppDatabase: vi.fn(),
  getCategoryMetrics: vi.fn().mockResolvedValue([]),
  getSmartSwitchArchiveInventory: vi.fn().mockResolvedValue([]),
  getSmartSwitchItemMetrics: vi.fn().mockResolvedValue([]),
  getStructuredRecords: vi.fn().mockResolvedValue([]),
}));

describe("DataExplorer", () => {
  it("keeps WhatsApp decrypt disabled until DB and key are present", async () => {
    render(<DataExplorer />);

    expect(await screen.findByRole("button", { name: "Decrypt locally" })).toBeDisabled();
  });
});
