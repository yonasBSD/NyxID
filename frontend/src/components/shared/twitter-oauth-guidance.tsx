import { Button } from "@/components/ui/button";
import { copyToClipboard } from "@/lib/utils";
import { Copy, ExternalLink } from "lucide-react";
import { toast } from "sonner";

function isTwitterOAuthSlug(slug: string): boolean {
  return slug === "twitter" || slug === "api-twitter";
}

function getApiOrigin(): string {
  const configured =
    (import.meta.env.VITE_BACKEND_URL as string | undefined) ??
    (import.meta.env.VITE_API_URL as string | undefined) ??
    "";

  const url = new URL(
    configured.trim() || window.location.origin,
    window.location.origin,
  );
  const path = url.pathname.replace(/\/api\/v1\/?$/, "").replace(/\/+$/, "");
  return `${url.origin}${path}`;
}

function providerCallbackUrl(): string {
  return `${getApiOrigin()}/api/v1/providers/callback`;
}

export function TwitterOAuthGuidance({
  slug,
}: {
  readonly slug: string;
}) {
  if (!isTwitterOAuthSlug(slug)) return null;

  const callbackUrl = providerCallbackUrl();

  function handleCopy() {
    void copyToClipboard(callbackUrl).then(() => {
      toast.success("Callback URL copied");
    });
  }

  return (
    <div className="space-y-3 rounded-lg border border-border bg-muted/40 p-3">
      <div className="space-y-1">
        <p className="text-xs font-medium text-foreground">
          Twitter / X OAuth setup
        </p>
        <p className="text-xs text-muted-foreground">
          This integration requires an X app with OAuth 2.0 enabled in{" "}
          <strong>User authentication settings</strong> in X Developer Console.
          Configure the callback URL below as one of your app&apos;s redirect
          URIs.
        </p>
      </div>

      <div className="space-y-1.5">
        <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          Callback URL
        </p>
        <div className="relative">
          <pre className="overflow-x-auto rounded-md bg-background p-2 pr-10 font-mono text-[11px] leading-relaxed">
            {callbackUrl}
          </pre>
          <Button
            type="button"
            size="icon"
            variant="ghost"
            className="absolute right-1.5 top-1.5 h-7 w-7"
            onClick={handleCopy}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      <a
        href="https://developer.x.com/en/portal/dashboard"
        target="_blank"
        rel="noopener noreferrer"
        className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
      >
        Where do I get Client ID and Client Secret? Open Keys &amp; Tokens in X
        Developer Console
        <ExternalLink className="h-3 w-3" />
      </a>
    </div>
  );
}
