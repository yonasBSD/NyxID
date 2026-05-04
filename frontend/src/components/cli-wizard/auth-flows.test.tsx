import { describe, expect, it, vi } from "vitest";
import {
  isTerminalAuthFailureStatus,
  pollOAuthKeyUntilActive,
} from "./auth-flow-polling";

describe("cli wizard auth flows", () => {
  it.each(["revoked", "failed", "expired"] as const)(
    "treats %s as a terminal auth failure status",
    (status) => {
      expect(isTerminalAuthFailureStatus(status)).toBe(true);
    },
  );

  it("does not treat active or pending_auth as terminal auth failures", () => {
    expect(isTerminalAuthFailureStatus("active")).toBe(false);
    expect(isTerminalAuthFailureStatus("pending_auth")).toBe(false);
    expect(isTerminalAuthFailureStatus(undefined)).toBe(false);
  });

  it("stops OAuth polling when the placeholder reaches a terminal failure", async () => {
    const getKey = vi
      .fn()
      .mockResolvedValueOnce({ status: "pending_auth" })
      .mockResolvedValueOnce({ status: "revoked" });
    const completeWithKey = vi.fn();
    const onTerminalFailure = vi.fn();
    const onTimeout = vi.fn();
    const sleepMs = vi.fn().mockResolvedValue(undefined);

    await pollOAuthKeyUntilActive({
      keyId: "key-1",
      getKey,
      completeWithKey,
      isCancelled: () => false,
      onTerminalFailure,
      onTimeout,
      sleepMs,
    });

    expect(getKey).toHaveBeenCalledTimes(2);
    expect(completeWithKey).not.toHaveBeenCalled();
    expect(onTerminalFailure).toHaveBeenCalledWith("revoked");
    expect(onTimeout).not.toHaveBeenCalled();
  });

  it("completes OAuth polling when the placeholder becomes active", async () => {
    const getKey = vi.fn().mockResolvedValue({ status: "active" });
    const completeWithKey = vi.fn().mockResolvedValue(undefined);
    const onTerminalFailure = vi.fn();
    const onTimeout = vi.fn();

    await pollOAuthKeyUntilActive({
      keyId: "key-1",
      getKey,
      completeWithKey,
      isCancelled: () => false,
      onTerminalFailure,
      onTimeout,
      sleepMs: vi.fn().mockResolvedValue(undefined),
    });

    expect(completeWithKey).toHaveBeenCalledWith("key-1");
    expect(onTerminalFailure).not.toHaveBeenCalled();
    expect(onTimeout).not.toHaveBeenCalled();
  });
});
