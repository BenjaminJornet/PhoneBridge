import { describe, expect, it } from "vitest";
import { mapWhatsAppError, shortenPath } from "./ux";

describe("shortenPath", () => {
  it("keeps empty and short paths readable", () => {
    expect(shortenPath("")).toBe("Not selected yet");
    expect(shortenPath("~/Library")).toBe("~/Library");
  });

  it("shortens long paths to the last three segments", () => {
    expect(shortenPath("/Users/example/Backups/Phone/DCIM")).toBe(".../Backups/Phone/DCIM");
  });
});

describe("mapWhatsAppError", () => {
  it("turns backend key errors into actionable copy", () => {
    expect(mapWhatsAppError("crypt14 requires a 131-byte key payload")).toContain("full WhatsApp key file");
    expect(mapWhatsAppError("WhatsApp hex key must be 64 hexadecimal characters")).toContain("64 hexadecimal");
  });
});
