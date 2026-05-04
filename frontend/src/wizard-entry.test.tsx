import { describe, expect, it } from "vitest";

import { shouldShowDisconnectBanner } from "./wizard-entry";

describe("shouldShowDisconnectBanner", () => {
  it("returns false when the CLI is still connected", () => {
    expect(shouldShowDisconnectBanner("claimed", false)).toBe(false);
  });

  it.each(["claimed", "secret", "acking"] as const)(
    "returns true for disconnected non-terminal phase %s",
    (phase) => {
      expect(shouldShowDisconnectBanner(phase, true)).toBe(true);
    },
  );

  it.each(["done", "cancelled"] as const)(
    "returns false for disconnected terminal phase %s",
    (phase) => {
      expect(shouldShowDisconnectBanner(phase, true)).toBe(false);
    },
  );
});
