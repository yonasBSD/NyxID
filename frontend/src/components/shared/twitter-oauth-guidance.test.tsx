import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TwitterOAuthGuidance } from "./twitter-oauth-guidance";

const mocks = vi.hoisted(() => ({
  copyToClipboard: vi.fn(),
  getApiBaseUrl: vi.fn(),
  toastSuccess: vi.fn(),
}));

vi.mock("@/lib/api-client", () => ({
  getApiBaseUrl: mocks.getApiBaseUrl,
}));

vi.mock("@/lib/utils", async () => {
  const actual = await vi.importActual<typeof import("@/lib/utils")>(
    "@/lib/utils",
  );
  return {
    ...actual,
    copyToClipboard: mocks.copyToClipboard,
  };
});

vi.mock("sonner", () => ({
  toast: {
    success: mocks.toastSuccess,
  },
}));

describe("TwitterOAuthGuidance", () => {
  beforeEach(() => {
    mocks.copyToClipboard.mockReset();
    mocks.copyToClipboard.mockResolvedValue(undefined);
    mocks.getApiBaseUrl.mockReset();
    mocks.toastSuccess.mockReset();
  });

  it("renders a copyable provider callback URL from the API base URL", async () => {
    const user = userEvent.setup();
    mocks.getApiBaseUrl.mockReturnValue(
      "https://nyx-api.chrono-ai.fun/api/v1",
    );

    render(<TwitterOAuthGuidance slug="twitter" />);

    const callbackUrl =
      "https://nyx-api.chrono-ai.fun/api/v1/providers/callback";
    expect(screen.getByText(callbackUrl)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Copy callback URL" }));

    expect(mocks.copyToClipboard).toHaveBeenCalledWith(callbackUrl);
    await waitFor(() => {
      expect(mocks.toastSuccess).toHaveBeenCalledWith("Callback URL copied");
    });
  });

  it("renders a user-facing fallback when the API base URL is unavailable", () => {
    mocks.getApiBaseUrl.mockReturnValue(null);

    render(<TwitterOAuthGuidance slug="api-twitter" />);

    expect(
      screen.getByText(
        /Callback URL not yet available\. Please contact your NyxID admin/,
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/VITE_BACKEND_URL/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Copy callback URL" }),
    ).not.toBeInTheDocument();
  });

  it("does not render for non-Twitter providers", () => {
    mocks.getApiBaseUrl.mockReturnValue(
      "https://nyx-api.chrono-ai.fun/api/v1",
    );

    const { container } = render(<TwitterOAuthGuidance slug="github" />);

    expect(container).toBeEmptyDOMElement();
    expect(mocks.getApiBaseUrl).not.toHaveBeenCalled();
  });
});
