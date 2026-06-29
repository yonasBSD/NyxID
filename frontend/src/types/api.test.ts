import { describe, expect, it } from "vitest";
import { isBillingAvailable } from "./api";

describe("isBillingAvailable", () => {
  it("fails closed when user capabilities are absent", () => {
    expect(isBillingAvailable(null)).toBe(false);
    expect(isBillingAvailable({})).toBe(false);
  });

  it("returns false when billing is explicitly unavailable", () => {
    expect(
      isBillingAvailable({
        capabilities: { billing_available: false },
      }),
    ).toBe(false);
  });

  it("returns true only when the backend marks billing available", () => {
    expect(
      isBillingAvailable({
        capabilities: { billing_available: true },
      }),
    ).toBe(true);
  });
});
