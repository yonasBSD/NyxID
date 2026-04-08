import { useState, useRef } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useNavigate, Link } from "@tanstack/react-router";
import { registerSchema, type RegisterFormData } from "@/schemas/auth";
import { useRegister } from "@/hooks/use-auth";
import { usePublicConfig } from "@/hooks/use-public-config";
import { ApiError } from "@/lib/api-client";
import { openExternal } from "@/lib/navigation";
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";

function getPasswordStrength(password: string): {
  score: number;
  label: string;
  color: string;
} {
  let score = 0;
  if (password.length >= 8) score += 1;
  if (password.length >= 12) score += 1;
  if (/[A-Z]/.test(password)) score += 1;
  if (/[a-z]/.test(password)) score += 1;
  if (/[0-9]/.test(password)) score += 1;
  if (/[^A-Za-z0-9]/.test(password)) score += 1;

  if (score <= 2) return { score, label: "Weak", color: "bg-destructive" };
  if (score <= 4) return { score, label: "Fair", color: "bg-amber-400" };
  return { score, label: "Strong", color: "bg-emerald-400" };
}

const INVITE_PATTERN = /^NYX-[A-Z0-9]{8,}$/;

const SOCIAL_PROVIDERS = [
  {
    id: "google",
    label: "Continue with Google",
    icon: (
      <svg className="h-4 w-4" viewBox="0 0 24 24">
        <path
          d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 01-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"
          fill="#4285F4"
        />
        <path
          d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
          fill="#34A853"
        />
        <path
          d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
          fill="#FBBC05"
        />
        <path
          d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
          fill="#EA4335"
        />
      </svg>
    ),
  },
  {
    id: "github",
    label: "Continue with GitHub",
    icon: (
      <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
        <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
      </svg>
    ),
  },
  {
    id: "apple",
    label: "Continue with Apple",
    icon: (
      <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
        <path d="M17.05 20.28c-.98.95-2.05.88-3.08.4-1.09-.5-2.08-.48-3.24 0-1.44.62-2.2.44-3.06-.4C2.79 15.25 3.51 7.59 9.05 7.31c1.35.07 2.29.74 3.08.8 1.18-.24 2.31-.93 3.57-.84 1.51.12 2.65.72 3.4 1.8-3.12 1.87-2.38 5.98.48 7.13-.57 1.5-1.31 2.99-2.54 4.09zM12.03 7.25c-.15-2.23 1.66-4.07 3.74-4.25.29 2.58-2.34 4.5-3.74 4.25z" />
      </svg>
    ),
  },
] as const;

interface RegisterFormProps {
  readonly returnTo?: string;
}

export function RegisterForm({ returnTo }: RegisterFormProps) {
  const [showEmailPanel, setShowEmailPanel] = useState(false);
  const navigate = useNavigate();
  const registerMutation = useRegister();
  const { data: config } = usePublicConfig();
  const nameInputRef = useRef<HTMLInputElement>(null);
  const inviteInputRef = useRef<HTMLInputElement>(null);

  const form = useForm<RegisterFormData>({
    resolver: zodResolver(registerSchema),
    defaultValues: {
      inviteCode: "",
      name: "",
      email: "",
      password: "",
      confirmPassword: "",
    },
  });

  const inviteCode = form.watch("inviteCode");
  const password = form.watch("password");
  const strength = getPasswordStrength(password);
  const isInviteValid = INVITE_PATTERN.test(inviteCode.trim().toUpperCase());

  const enabledProviders = config
    ? SOCIAL_PROVIDERS.filter((p) =>
        config.social_providers.includes(p.id),
      )
    : SOCIAL_PROVIDERS;

  function handleSocialLogin(providerId: string) {
    const params = new URLSearchParams();
    if (returnTo) params.set("return_to", returnTo);
    const code = inviteCode.trim().toUpperCase();
    if (code) params.set("invite_code", code);
    const qs = params.toString();
    const url = `${window.location.origin}/api/v1/auth/social/${encodeURIComponent(providerId)}${qs ? `?${qs}` : ""}`;
    void openExternal(url);
  }

  function slideToEmail() {
    setShowEmailPanel(true);
    setTimeout(() => nameInputRef.current?.focus(), 380);
  }

  function slideBack() {
    setShowEmailPanel(false);
    setTimeout(() => {
      inviteInputRef.current?.focus();
      inviteInputRef.current?.select();
    }, 380);
  }

  async function onSubmit(data: RegisterFormData) {
    try {
      const result = await registerMutation.mutateAsync({
        name: data.name,
        email: data.email,
        password: data.password,
        invite_code: data.inviteCode,
      });
      toast.success(result.message || "Account created successfully");
      void navigate({
        to: "/login" as string,
        search: returnTo ? { return_to: returnTo } : {},
      });
    } catch (error) {
      if (error instanceof ApiError) {
        form.setError("root", { message: error.message });
      } else {
        form.setError("root", {
          message: "An unexpected error occurred. Please try again.",
        });
      }
    }
  }

  return (
    <div className="-m-8 overflow-hidden rounded-[10px]">
      <div
        className="flex w-[200%] transition-transform duration-350 ease-[cubic-bezier(0.4,0,0.2,1)]"
        style={{
          transform: showEmailPanel ? "translateX(-50%)" : "translateX(0)",
        }}
      >
        {/* Panel 1: Method Selection */}
        <div className="w-1/2 shrink-0 px-7 pt-8 pb-7">
          <div className="mb-7 text-center">
            <h1 className="font-display text-2xl font-semibold tracking-tight [background:linear-gradient(180deg,#fff_30%,#a0a0a8_100%)] bg-clip-text text-transparent">
              Create your account
            </h1>
            <p className="mt-1.5 text-[13px] text-text-tertiary">
              Start securing your digital identity
            </p>
          </div>

          {/* Step 1: Invite Code */}
          <div className="relative mb-6 pl-9">
            <div className="absolute left-[11px] top-7 bottom-[-12px] w-px bg-gradient-to-b from-violet-500/10 to-transparent" />
            <div
              className={`absolute left-0 top-0 flex h-6 w-6 items-center justify-center rounded-full border text-[11px] font-semibold transition-colors ${
                isInviteValid
                  ? "border-transparent bg-gradient-to-br from-violet-400 via-violet-500 to-violet-600 text-white"
                  : "border-violet-500/15 bg-violet-500/10 text-violet-400"
              }`}
            >
              {isInviteValid ? (
                <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                  <path
                    d="M3 8.5L6.5 12L13 4"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
              ) : (
                "1"
              )}
            </div>
            <p className="mb-3 text-[13px] font-medium leading-6 text-muted-foreground">
              Enter your invite code
            </p>
            <FormField
              control={form.control}
              name="inviteCode"
              render={({ field }) => (
                <FormItem>
                  <FormControl>
                    <Input
                      placeholder="NYX-XXXXXXXX"
                      autoComplete="off"
                      spellCheck={false}
                      className={`h-12 font-mono text-base tracking-wider ${
                        isInviteValid
                          ? "border-emerald-400/40 shadow-[0_0_0_3px_rgba(52,211,153,0.08)]"
                          : ""
                      }`}
                      {...field}
                      ref={(el) => {
                        field.ref(el);
                        (
                          inviteInputRef as React.MutableRefObject<HTMLInputElement | null>
                        ).current = el;
                      }}
                      onChange={(e) =>
                        field.onChange(e.target.value.toUpperCase())
                      }
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          </div>

          {/* Step 2: Choose Method */}
          <div className="relative pl-9">
            <div className="absolute left-0 top-0 flex h-6 w-6 items-center justify-center rounded-full border border-violet-500/15 bg-violet-500/10 text-[11px] font-semibold text-violet-400">
              2
            </div>
            <p className="mb-3 text-[13px] font-medium leading-6 text-muted-foreground">
              Choose how to sign up
            </p>

            <div className="flex flex-col gap-2">
              {enabledProviders.map((provider) => (
                <button
                  key={provider.id}
                  type="button"
                  onClick={() => handleSocialLogin(provider.id)}
                  className="flex h-[46px] items-center gap-3 rounded-lg border border-border bg-background px-4 text-[13.5px] font-medium text-foreground transition-colors hover:border-border/80 hover:bg-white/[0.03] active:scale-[0.99]"
                >
                  <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-white/[0.06]">
                    {provider.icon}
                  </span>
                  {provider.label}
                  <span className="ml-auto text-sm text-muted-foreground">
                    &rsaquo;
                  </span>
                </button>
              ))}

              <button
                type="button"
                onClick={slideToEmail}
                className="flex h-[46px] items-center gap-3 rounded-lg border border-border bg-background px-4 text-[13.5px] font-medium text-foreground transition-colors hover:border-border/80 hover:bg-white/[0.03] active:scale-[0.99]"
              >
                <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-violet-500/10">
                  <svg
                    className="h-4 w-4 text-violet-400"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.8"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <rect x="2" y="4" width="20" height="16" rx="2" />
                    <path d="M22 7l-10 6L2 7" />
                  </svg>
                </span>
                Continue with Email
                <span className="ml-auto text-sm text-muted-foreground">
                  &rsaquo;
                </span>
              </button>
            </div>
          </div>

          {/* Footer */}
          <div className="mt-6 border-t border-border pt-5 text-center text-[13px] text-muted-foreground">
            Already have an account?{" "}
            <Link
              to="/login"
              search={returnTo ? { return_to: returnTo } : {}}
              className="font-medium text-violet-400 hover:text-violet-300"
            >
              Sign in
            </Link>
          </div>
        </div>

        {/* Panel 2: Email Registration */}
        <div
          className="w-1/2 shrink-0 px-7 pt-8 pb-7"
          onKeyDown={(e) => {
            if (e.key === "Escape") slideBack();
          }}
        >
          {/* Header with back button */}
          <div className="mb-6 flex items-center gap-3">
            <button
              type="button"
              onClick={slideBack}
              className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-border bg-transparent text-muted-foreground transition-colors hover:border-border/80 hover:bg-white/[0.03] hover:text-foreground"
              aria-label="Back to sign-up methods"
            >
              <svg
                className="h-4 w-4"
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="M10 3L5 8l5 5" />
              </svg>
            </button>
            <div>
              <h2 className="text-lg font-semibold tracking-tight">
                Email registration
              </h2>
              <p className="text-xs text-muted-foreground">
                Create your account with email and password
              </p>
            </div>
          </div>

          {/* Invite code mirror */}
          <div className="mb-5 flex items-center gap-2.5 rounded-lg border border-violet-500/10 bg-violet-500/10 px-3.5 py-2.5">
            <svg
              className="h-3.5 w-3.5 shrink-0 text-violet-400"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <rect x="3" y="7" width="10" height="7" rx="1.5" />
              <path d="M5 7V5a3 3 0 016 0v2" />
            </svg>
            <span className="font-mono text-[13px] font-medium tracking-wider text-violet-400">
              {inviteCode.trim().toUpperCase() || "NYX-XXXXXXXX"}
            </span>
            <button
              type="button"
              onClick={slideBack}
              className="ml-auto border-0 bg-transparent text-[11px] font-medium text-muted-foreground transition-colors hover:text-foreground"
            >
              Edit
            </button>
          </div>

          {/* Email registration form */}
          <Form {...form}>
            <form
              onSubmit={form.handleSubmit(onSubmit)}
              className="flex flex-col gap-3.5"
            >
              {form.formState.errors.root && (
                <div
                  role="alert"
                  className="rounded-md bg-destructive/10 p-3 text-sm text-destructive"
                >
                  {form.formState.errors.root.message}
                </div>
              )}

              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel className="text-xs">Full Name</FormLabel>
                    <FormControl>
                      <Input
                        placeholder="John Doe"
                        autoComplete="name"
                        className="h-[42px] text-[13.5px]"
                        {...field}
                        ref={(el) => {
                          field.ref(el);
                          (
                            nameInputRef as React.MutableRefObject<HTMLInputElement | null>
                          ).current = el;
                        }}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="email"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel className="text-xs">Email</FormLabel>
                    <FormControl>
                      <Input
                        type="email"
                        placeholder="you@example.com"
                        autoComplete="email"
                        className="h-[42px] text-[13.5px]"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="password"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel className="text-xs">Password</FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        placeholder="Min 8 characters"
                        autoComplete="new-password"
                        className="h-[42px] text-[13.5px]"
                        {...field}
                      />
                    </FormControl>
                    {password.length > 0 && (
                      <div className="mt-1.5 space-y-0.5">
                        <div
                          className="flex gap-[3px]"
                          role="progressbar"
                          aria-valuenow={strength.score}
                          aria-valuemin={0}
                          aria-valuemax={6}
                          aria-label={`Password strength: ${strength.label}`}
                        >
                          {Array.from({ length: 6 }).map((_, i) => (
                            <div
                              key={`s-${String(i)}`}
                              className={`h-[2.5px] flex-1 rounded-full ${
                                i < strength.score
                                  ? strength.color
                                  : "bg-white/[0.06]"
                              }`}
                            />
                          ))}
                        </div>
                        <p className="text-[11px] text-muted-foreground">
                          {strength.label}
                        </p>
                      </div>
                    )}
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="confirmPassword"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel className="text-xs">Confirm Password</FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        placeholder="Re-enter your password"
                        autoComplete="new-password"
                        className="h-[42px] text-[13.5px]"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <Button
                type="submit"
                className="mt-0.5 h-11 w-full bg-gradient-to-br from-violet-400 via-violet-500 to-violet-600 text-sm font-medium shadow-[0_2px_12px_rgba(139,92,246,0.2)] hover:opacity-90 hover:shadow-[0_4px_20px_rgba(139,92,246,0.3)]"
                isLoading={registerMutation.isPending}
              >
                Create Account
              </Button>
            </form>
          </Form>

          {/* Footer */}
          <div className="mt-6 border-t border-border pt-5 text-center text-[13px] text-muted-foreground">
            Already have an account?{" "}
            <Link
              to="/login"
              search={returnTo ? { return_to: returnTo } : {}}
              className="font-medium text-violet-400 hover:text-violet-300"
            >
              Sign in
            </Link>
          </div>
        </div>
      </div>
    </div>
  );
}
