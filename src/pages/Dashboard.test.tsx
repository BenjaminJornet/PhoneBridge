import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import Dashboard from "./Dashboard";

vi.mock("../lib/api", () => ({
  getCategoryMetrics: vi.fn().mockResolvedValue([]),
  scanBackupSources: vi.fn().mockResolvedValue([]),
}));

describe("Dashboard", () => {
  it("shows the first-run import CTA when the library is empty", async () => {
    render(<Dashboard onNavigate={vi.fn()} />);

    expect(await screen.findByText("Nothing has been imported yet.")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /start guided import|import data/i }).length).toBeGreaterThan(0);
  });
});
