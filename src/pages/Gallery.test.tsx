import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import Gallery from "./Gallery";

const listIndexedFiles = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (value: string) => value,
}));

vi.mock("../lib/api", () => ({
  listIndexedFiles: (...args: unknown[]) => listIndexedFiles(...args),
}));

describe("Gallery", () => {
  it("shows the empty-state import action", async () => {
    const onImport = vi.fn();
    listIndexedFiles.mockResolvedValueOnce([]);

    render(<Gallery onImport={onImport} />);

    const button = await screen.findByRole("button", { name: "Start guided import" });
    await userEvent.click(button);
    expect(onImport).toHaveBeenCalledTimes(1);
  });

  it("loads another page when requested", async () => {
    listIndexedFiles
      .mockResolvedValueOnce(Array.from({ length: 120 }, (_, index) => ({
        id: index + 1,
        absolutePath: `/tmp/${index + 1}.jpg`,
        relativePath: `Photo/DCIM/${index + 1}.jpg`,
        category: "photo",
        source: "DCIM",
        extension: "jpg",
        sizeBytes: 10,
      })))
      .mockResolvedValueOnce([
        {
          id: 121,
          absolutePath: "/tmp/121.jpg",
          relativePath: "Photo/DCIM/121.jpg",
          category: "photo",
          source: "DCIM",
          extension: "jpg",
          sizeBytes: 10,
        },
      ]);

    render(<Gallery onImport={vi.fn()} />);

    const loadMore = await screen.findByRole("button", { name: "Load more" });
    await userEvent.click(loadMore);

    await waitFor(() => {
      expect(screen.getByText("Photo/DCIM/121.jpg")).toBeInTheDocument();
    });
  });
});
