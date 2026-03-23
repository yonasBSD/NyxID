import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { LoginForm } from "./login-form";

// Stub out hooks and routing used by LoginForm
const mocks = vi.hoisted(() => ({
  useNavigate: vi.fn(() => vi.fn()),
  useLogin: vi.fn(() => ({ mutateAsync: vi.fn(), isPending: false })),
  usePublicConfig: vi.fn(() => ({ data: null })),
}));

vi.mock("@tanstack/react-router", () => ({
  useNavigate: mocks.useNavigate,
  Link: ({ children, ...props }: Record<string, unknown>) => (
    <a href={props.to as string}>{children as React.ReactNode}</a>
  ),
}));

vi.mock("@/hooks/use-auth", () => ({
  useLogin: mocks.useLogin,
}));

vi.mock("@/hooks/use-public-config", () => ({
  usePublicConfig: mocks.usePublicConfig,
}));

vi.mock("@/lib/navigation", () => ({
  openExternal: vi.fn(),
}));

describe("LoginForm social error", () => {
  it("displays the social_auth_conflict error message", () => {
    render(<LoginForm socialError="social_auth_conflict" />);

    const alert = screen.getByTestId("social-error");
    expect(alert).toBeInTheDocument();
    expect(alert).toHaveTextContent(
      "This email is already linked to another sign-in method",
    );
  });

  it("displays a fallback message for unknown error keys", () => {
    render(<LoginForm socialError="some_unknown_error" />);

    const alert = screen.getByTestId("social-error");
    expect(alert).toBeInTheDocument();
    expect(alert).toHaveTextContent("Social sign-in failed. Please try again.");
  });

  it("does not render the error alert when no socialError is provided", () => {
    render(<LoginForm />);

    expect(screen.queryByTestId("social-error")).not.toBeInTheDocument();
  });

  it("displays the social_auth_no_email error message", () => {
    render(<LoginForm socialError="social_auth_no_email" />);

    const alert = screen.getByTestId("social-error");
    expect(alert).toHaveTextContent(
      "We couldn't retrieve an email address from your social account",
    );
  });

  it("displays the social_auth_deactivated error message", () => {
    render(<LoginForm socialError="social_auth_deactivated" />);

    const alert = screen.getByTestId("social-error");
    expect(alert).toHaveTextContent(
      "Your account has been deactivated",
    );
  });
});
