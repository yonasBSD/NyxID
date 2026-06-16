import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { PropsWithChildren } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useApproveDevice, useOnboardDevice } from "./use-devices";

const { mockPost } = vi.hoisted(() => ({
  mockPost: vi.fn(),
}));

vi.mock("@/lib/api-client", () => ({
  api: { post: mockPost },
}));

function wrapperFactory() {
  const queryClient = new QueryClient({
    defaultOptions: {
      mutations: { retry: false },
      queries: { retry: false },
    },
  });
  const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");

  return {
    invalidateSpy,
    wrapper: ({ children }: PropsWithChildren) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    ),
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("useApproveDevice", () => {
  it("posts the approval payload to /devices/code/approve and parses the response", async () => {
    mockPost.mockResolvedValue({
      device_label: "Hall camera",
      hw_id: "esp32-aabbcc",
      api_key_id: "api-key-1",
      node_id: "node-1",
      owner_user_id: "user-1",
      org_id: null,
    });
    const { wrapper } = wrapperFactory();
    const { result } = renderHook(() => useApproveDevice(), { wrapper });

    const response = await result.current.mutateAsync({
      user_code: "ABCD-EFGH-JKLM",
      org_id: undefined,
      label: "Hall camera",
      default_services: ["svc-1"],
    });

    expect(mockPost).toHaveBeenCalledWith("/devices/code/approve", {
      user_code: "ABCD-EFGH-JKLM",
      label: "Hall camera",
      default_services: ["svc-1"],
    });
    expect(response.device_label).toBe("Hall camera");
  });

  it("invalidates keys, api-keys, and nodes after approval succeeds", async () => {
    mockPost.mockResolvedValue({
      device_label: "Hall camera",
      hw_id: "esp32-aabbcc",
      api_key_id: "api-key-1",
      node_id: "node-1",
      owner_user_id: "user-1",
      org_id: null,
    });
    const { invalidateSpy, wrapper } = wrapperFactory();
    const { result } = renderHook(() => useApproveDevice(), { wrapper });

    await result.current.mutateAsync({
      user_code: "ABCD-EFGH-JKLM",
      org_id: undefined,
    });

    await waitFor(() =>
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["nodes"] }),
    );
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["api-keys"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["keys"] });
  });
});

describe("useOnboardDevice", () => {
  it("posts the onboard payload to /devices/onboard and parses the response", async () => {
    mockPost.mockResolvedValue({
      qr_payload: "nyxprov://bootstrap?token=nyx_obt_secret",
      bootstrap_id: "boot-1",
      label: "Kitchen Camera",
      expires_in: 900,
      expires_at: "2026-06-16T12:15:00Z",
    });
    const { wrapper } = wrapperFactory();
    const { result } = renderHook(() => useOnboardDevice(), { wrapper });

    const response = await result.current.mutateAsync({
      org_id: undefined,
      label: "Kitchen Camera",
      default_services: ["svc-1"],
    });

    expect(mockPost).toHaveBeenCalledWith("/devices/onboard", {
      label: "Kitchen Camera",
      default_services: ["svc-1"],
    });
    expect(response.qr_payload).toBe("nyxprov://bootstrap?token=nyx_obt_secret");
  });

  it("does not invalidate key or node lists until the device redeems the bootstrap", async () => {
    mockPost.mockResolvedValue({
      qr_payload: "nyxprov://bootstrap?token=nyx_obt_secret",
      bootstrap_id: "boot-1",
      label: "Kitchen Camera",
      expires_in: 900,
      expires_at: "2026-06-16T12:15:00Z",
    });
    const { invalidateSpy, wrapper } = wrapperFactory();
    const { result } = renderHook(() => useOnboardDevice(), { wrapper });

    await result.current.mutateAsync({
      org_id: undefined,
      label: "Kitchen Camera",
    });

    expect(invalidateSpy).not.toHaveBeenCalled();
  });
});
