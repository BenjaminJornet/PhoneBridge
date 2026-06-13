import { describe, expect, it } from "vitest";
import { formatBytes, formatCount } from "./format";

describe("formatBytes", () => {
  it("formats zero and invalid values as bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });

  it("formats byte and binary units", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 ** 2 * 1.5)).toBe("1.5 MB");
  });
});

describe("formatCount", () => {
  it("formats counts using stable en-US separators", () => {
    expect(formatCount(1234567)).toBe("1,234,567");
  });
});
