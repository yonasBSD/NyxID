import { LoginForm } from "@/components/auth/login-form";
import { MfaVerifyForm } from "@/components/auth/mfa-verify-form";
import { useAuthStore } from "@/stores/auth-store";

export function LoginPage() {
  const mfaRequired = useAuthStore((s) => s.mfaRequired);

  // Read return_to and error from the URL (set by the backend OAuth flows)
  const params = new URLSearchParams(window.location.search);
  const returnTo = params.get("return_to") ?? undefined;
  const socialError = params.get("error") ?? undefined;

  if (mfaRequired) {
    return <MfaVerifyForm returnTo={returnTo} />;
  }

  return <LoginForm returnTo={returnTo} socialError={socialError} />;
}
