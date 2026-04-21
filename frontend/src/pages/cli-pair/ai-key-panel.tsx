// ai-key (service-add) pairing panel.
//
// Supports all three provider shapes mirroring
// `cli/src/wizard/assets/wizard.js`:
//   - api_key / bearer — inline credential input, single `POST /keys`
//   - oauth            — placeholder key + popup + poll-until-active
//   - device_code      — placeholder key + code display + poll
//
// OAuth/device-code sub-flows live in `./ai-key-auth-flows.tsx` to
// keep this file focused on catalog lookup + dispatch.

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError, api } from "@/lib/api-client";
import { ExternalLink } from "lucide-react";
import type { AiKeyPrefill } from "./types";
import { DeviceCodeFlow, OAuthFlow } from "./ai-key-auth-flows";
import { reservePairingAction, withRewindOnError } from "./reserve-action";

/**
 * Catalog response shape we care about. Fields beyond these are
 * ignored — we don't narrow further because the catalog endpoint
 * returns ~30 fields of metadata and most aren't relevant here.
 */
/**
 * Per-field schema for `token_exchange` catalog entries. Matches the
 * backend's `CredentialFieldSpec`. Multiple fields are collected into
 * a JSON-encoded blob and sent as a single `credential` string on
 * `POST /keys`.
 */
interface CredentialFieldSpec {
  readonly name: string;
  readonly label: string;
  readonly placeholder?: string;
  readonly secret?: boolean;
}

interface CatalogEntryShape {
  readonly slug: string;
  readonly name: string;
  readonly description?: string;
  readonly base_url: string;
  /**
   * Downstream credential injection mode — `bearer`, `header`, `query`,
   * `basic`, `bot_bearer`, etc. Does NOT decide the pairing flow; use
   * `provider_type` for that.
   */
  readonly auth_method: string;
  /**
   * How the user *obtains* the credential — `oauth2`, `device_code`,
   * `api_key`. This is what the wizard routes on.
   */
  readonly provider_type?: string;
  readonly service_type: string;
  readonly requires_credential: boolean;
  readonly requires_gateway_url: boolean;
  /** Required by OAuth / device-code sub-flows. */
  readonly provider_config_id?: string;
  /**
   * `system` / `user` / `both` — whether this provider uses the
   * NyxID-admin-registered OAuth app, the user's own OAuth app, or
   * accepts both. When `user` or `both`, the OAuth pre-step collects
   * client_id / client_secret before the popup.
   */
  readonly credential_mode?: string;
  /** Admin-provided docs URL shown inside the user-OAuth-app step. */
  readonly documentation_url?: string;
  /**
   * Multi-field credential schema. When present and non-empty, this
   * catalog entry uses the `token_exchange` flow — render one form
   * input per field, collect a JSON object, and send as the
   * `credential` string. Used by Lark, Feishu, etc.
   */
  readonly token_exchange_credential_fields?: readonly CredentialFieldSpec[];
  readonly icon_url?: string;
  readonly api_key_url?: string;
  readonly api_key_instructions?: string;
}

/** Shape of `POST /keys` response we care about. */
interface UserServiceCreated {
  readonly id: string;
  readonly slug: string;
  readonly label: string;
  readonly endpoint_url: string;
}

export interface AiKeyPairingSuccess {
  readonly kind: "ai-key";
  readonly service_id: string;
  readonly slug: string;
  readonly label: string;
}

interface AiKeyConfirmProps {
  readonly prefill: AiKeyPrefill;
  /**
   * Pairing record id. Used to cancel the pairing server-side when
   * the user navigates away to the main Keys page (unsupported flow
   * fallback). Without this, the CLI keeps polling until TTL even
   * though the browser has given up on completing here.
   */
  readonly pairingId: string;
  readonly onSuccess: (result: AiKeyPairingSuccess) => void;
}

/**
 * Flow classification. Mirrors the local wizard's `authShape` in
 * `cli/src/wizard/assets/wizard.js` — which routes on
 * `provider_type`, NOT `auth_method`. `auth_method` is the downstream
 * credential-injection mode (bearer vs header vs query vs basic, etc.);
 * an `auth_method: "bearer"` OAuth provider like GitHub is still
 * fundamentally an OAuth flow, and an `auth_method: "header"` api-key
 * provider like xAI still needs the paste-a-key UI.
 */
type AuthShape =
  | "no-auth"
  | "token-exchange"
  | "api-key"
  | "oauth"
  | "device-code"
  | "other";

function classifyFlow(entry: CatalogEntryShape): AuthShape {
  // OAuth / device-code win even if the entry also declares a credential
  // schema (provider_type is the source of truth for the auth flow).
  const pt = (entry.provider_type ?? "").toLowerCase();
  if (pt === "oauth2") return "oauth";
  if (pt === "device_code") return "device-code";
  // No-auth services (public proxies, non-credential downstreams) —
  // we just POST /keys and move on. Same shape the local wizard
  // calls "no-auth" / "1-click connect".
  if (entry.requires_credential === false) return "no-auth";
  // Token-exchange providers (Lark, Feishu, etc.) collect multiple
  // fields (app_id + app_secret, etc.) that the backend combines into
  // a JSON-encoded credential string. Check BEFORE "api-key" because
  // token_exchange entries also set `requires_credential: true`.
  if (
    Array.isArray(entry.token_exchange_credential_fields) &&
    entry.token_exchange_credential_fields.length > 0
  ) {
    return "token-exchange";
  }
  if (entry.requires_credential) return "api-key";
  return "other";
}

export function AiKeyConfirm({ prefill, pairingId, onSuccess }: AiKeyConfirmProps) {
  const [slug, setSlug] = useState(prefill.slug ?? "");
  const trimmedSlug = slug.trim();

  const {
    data: entry,
    isLoading: entryLoading,
    error: entryError,
  } = useQuery({
    queryKey: ["cli-pair", "catalog", trimmedSlug],
    queryFn: async (): Promise<CatalogEntryShape> => {
      return api.get<CatalogEntryShape>(
        `/catalog/${encodeURIComponent(trimmedSlug)}`,
      );
    },
    enabled: Boolean(trimmedSlug),
  });

  const entryErrorMessage = (() => {
    if (!entryError) return null;
    if (entryError instanceof ApiError) return entryError.message;
    return `Couldn't load catalog entry "${trimmedSlug}".`;
  })();

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h2 className="font-display text-xl font-semibold">
          Add an external service
        </h2>
        <p className="text-sm text-muted-foreground">
          Your CLI wants to add <strong>{trimmedSlug || "a service"}</strong>{" "}
          to NyxID. Confirm the details here.
        </p>
      </div>

      {!trimmedSlug ? (
        <SlugPicker onSubmit={setSlug} />
      ) : entryLoading ? (
        <Skeleton className="h-24 w-full" />
      ) : entryErrorMessage ? (
        // A bad slug or missing catalog entry would otherwise strand
        // the user on an error screen with no way to recover. Render
        // the picker underneath the error so they can edit the slug
        // in place without re-running the CLI.
        <div className="flex flex-col gap-3">
          <ErrorLine message={entryErrorMessage} />
          <SlugPicker onSubmit={setSlug} />
        </div>
      ) : entry ? (
        <CatalogConfirmForm
          entry={entry}
          prefill={prefill}
          pairingId={pairingId}
          onSuccess={onSuccess}
        />
      ) : null}
    </div>
  );
}

// ── no-slug fallback ────────────────────────────────────────────────

function SlugPicker({ onSubmit }: { readonly onSubmit: (s: string) => void }) {
  const [s, setS] = useState("");
  return (
    <div className="flex flex-col gap-3">
      <p className="text-sm text-muted-foreground">
        Your CLI didn't send a catalog slug. Enter one to continue (try{" "}
        <code className="rounded bg-muted px-1 py-0.5 text-xs">llm-openai</code>
        ), or cancel and run <code>nyxid catalog list</code> first.
      </p>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="pair-aikey-slug">Catalog slug</Label>
        <Input
          id="pair-aikey-slug"
          value={s}
          onChange={(e) => {
            setS(e.target.value);
          }}
          placeholder="llm-openai"
          autoFocus
        />
      </div>
      <Button
        onClick={() => {
          onSubmit(s.trim());
        }}
        disabled={!s.trim()}
      >
        Continue
      </Button>
    </div>
  );
}

// ── per-entry confirm: renders the right inputs for the auth method ─

interface CatalogConfirmFormProps {
  readonly entry: CatalogEntryShape;
  readonly prefill: AiKeyPrefill;
  readonly pairingId: string;
  readonly onSuccess: (result: AiKeyPairingSuccess) => void;
}

function CatalogConfirmForm({
  entry,
  prefill,
  pairingId,
  onSuccess,
}: CatalogConfirmFormProps) {
  const shape = classifyFlow(entry);
  const [label, setLabel] = useState(prefill.label ?? entry.name);
  const [credential, setCredential] = useState("");
  const [endpointUrl, setEndpointUrl] = useState(prefill.endpoint_url ?? "");
  // Per-field state for token-exchange flows. Keyed by the spec
  // `name`; combined into a JSON-encoded `credential` string on
  // submit. One hook per entry so flipping between entries doesn't
  // leak values across providers.
  const [tokenFields, setTokenFields] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Set once the user clicks "Continue" for an OAuth / device-code
  // flow; drops the confirm UI and mounts the sub-flow panel. Reset
  // to `null` if the sub-flow reports cancel so the user can edit
  // label / retry.
  const [authFlowActive, setAuthFlowActive] = useState(false);

  async function submit() {
    setLoading(true);
    setError(null);
    try {
      const body: Record<string, unknown> = {
        service_slug: entry.slug,
        label,
      };
      if (shape === "token-exchange") {
        // Multi-field credential: validate required-ness and JSON-
        // encode. Mirror wizard.js submit path at wizard.js:723-734.
        const fields = entry.token_exchange_credential_fields ?? [];
        const creds: Record<string, string> = {};
        for (const f of fields) {
          const val = tokenFields[f.name]?.trim();
          if (!val) {
            setError(`${f.label || f.name} is required.`);
            setLoading(false);
            return;
          }
          creds[f.name] = val;
        }
        body.credential = JSON.stringify(creds);
      } else if (shape === "api-key" && entry.requires_credential) {
        body.credential = credential;
      }
      // `no-auth` / `oauth` / `device-code` skip credential entirely;
      // OAuth / device-code placeholder creation lives in the sub-
      // flow components and doesn't reach this branch.
      if (entry.requires_gateway_url || endpointUrl) {
        body.endpoint_url = endpointUrl;
      }
      if (prefill.via_node) {
        body.node_id = prefill.via_node;
      }

      // Reserve the destructive action server-side before creating
      // the service. Without this, a refresh after `POST /keys`
      // succeeded but before `AiKeyAckPanel` posts `/complete` lets
      // the pairing's confirm form fire again, creating a duplicate
      // UserService / UserApiKey and orphaning the first one from
      // the CLI's ack. Throws 409/404 in stale-tab scenarios so the
      // surrounding try/catch aborts before the duplicate POST.
      await reservePairingAction(pairingId);
      // Rewind the reservation if `POST /keys` fails (e.g. backend
      // rejects a malformed label or credential); the user can fix
      // the input in place and retry without re-running the CLI.
      const res = await withRewindOnError(pairingId, () =>
        api.post<UserServiceCreated>("/keys", body),
      );
      onSuccess({
        kind: "ai-key",
        service_id: res.id,
        slug: res.slug,
        label: res.label,
      });
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Failed to create service.");
    } finally {
      setLoading(false);
    }
  }

  // Both "unsupported" fallbacks below navigate the user away from
  // this page; if we don't explicitly cancel the pairing, the
  // waiting CLI keeps polling until its TTL expires (up to 15
  // minutes) and the main Keys page has no knowledge of the
  // pairing to complete it. Cancel first so the CLI's next poll
  // sees `Cancelled` and exits cleanly with a "please finish on
  // the web UI" message.
  async function cancelThenNavigate(href: string | null) {
    try {
      await api.post(
        `/cli-pairings/${encodeURIComponent(pairingId)}/cancel`,
        {},
      );
    } catch {
      // Fire-and-forget: if cancel fails (expired, already
      // cancelled, network), we still navigate. Worst case the CLI
      // times out the same way it did before this fix.
    }
    if (href) {
      window.location.assign(href);
    } else {
      window.history.back();
    }
  }

  // SSH services use a distinct `service add-ssh` command in the CLI
  // (certificate issuance, not credential binding). The local wizard
  // refuses SSH too — keep parity and tell the user the right command.
  if (entry.service_type === "ssh") {
    return (
      <div className="flex flex-col gap-3">
        <p className="rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm">
          {entry.name} is an SSH service. Use{" "}
          <code>nyxid service add-ssh</code> from your CLI instead
          (certificate-based auth, not a credential binding).
        </p>
        <Button
          variant="outline"
          onClick={() => void cancelThenNavigate(null)}
        >
          Go back
        </Button>
      </div>
    );
  }

  // Unknown auth shape or missing provider_config_id for OAuth /
  // device-code. Fall back to the regular Keys page where the user
  // can complete setup in a different UI.
  if (
    shape === "other" ||
    ((shape === "oauth" || shape === "device-code") && !entry.provider_config_id)
  ) {
    // `tab=services` puts the user on the right tab; `slug=...` is
    // consumed by `KeysPage` to auto-open `AddKeyDialog` with the
    // catalog entry pre-selected so the user doesn't have to hunt
    // for it in the grid.
    const href =
      `/keys?tab=services&slug=${encodeURIComponent(entry.slug)}`;
    return (
      <div className="flex flex-col gap-3">
        <p className="rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm">
          {entry.name} uses <code>{entry.auth_method}</code> auth,
          which isn't supported via remote pairing. Complete setup on
          the main Keys page. Your CLI will receive a cancel and
          print a "finish in browser" hint.
        </p>
        <Button
          onClick={() => void cancelThenNavigate(href)}
          className="justify-center gap-2"
        >
          Open Keys page
          <ExternalLink className="h-4 w-4" />
        </Button>
      </div>
    );
  }

  // OAuth / device-code: the confirm form collects label only, then
  // the flow handoff component drives placeholder creation, provider
  // auth, and polling.
  // Resolve the effective endpoint URL: the user's `--endpoint-url`
  // override (either via prefill from the CLI or the in-form input
  // for `requires_gateway_url` services) wins over the catalog
  // default. Both OAuth and device-code placeholder creation need
  // this or self-hosted services will bind to the wrong host.
  const effectiveEndpointUrl = endpointUrl.trim() || prefill.endpoint_url;

  if (authFlowActive && shape === "oauth" && entry.provider_config_id) {
    return (
      <OAuthFlow
        providerId={entry.provider_config_id}
        slug={entry.slug}
        label={label}
        nodeId={prefill.via_node}
        endpointUrl={effectiveEndpointUrl}
        pairingId={pairingId}
        credentialMode={entry.credential_mode}
        documentationUrl={entry.documentation_url}
        onSuccess={onSuccess}
        onCancel={() => {
          setAuthFlowActive(false);
        }}
      />
    );
  }
  if (authFlowActive && shape === "device-code" && entry.provider_config_id) {
    return (
      <DeviceCodeFlow
        providerId={entry.provider_config_id}
        slug={entry.slug}
        label={label}
        nodeId={prefill.via_node}
        endpointUrl={effectiveEndpointUrl}
        pairingId={pairingId}
        onSuccess={onSuccess}
        onCancel={() => {
          setAuthFlowActive(false);
        }}
      />
    );
  }

  const needsCredentialInput = shape === "api-key" && entry.requires_credential;
  const submitLabel = (() => {
    if (loading) return "Creating...";
    if (shape === "oauth") return "Continue with provider sign-in";
    if (shape === "device-code") return "Get device code";
    if (shape === "no-auth") return "Connect";
    return "Create service";
  })();

  // Submit is disabled until all required inputs are filled. The
  // per-shape rules mirror the validation inside `submit()`.
  const tokenExchangeComplete = (() => {
    if (shape !== "token-exchange") return true;
    const fields = entry.token_exchange_credential_fields ?? [];
    return fields.every((f) => (tokenFields[f.name] ?? "").trim().length > 0);
  })();
  const submitDisabled =
    loading ||
    !label.trim() ||
    (needsCredentialInput && !credential.trim()) ||
    (entry.requires_gateway_url && !endpointUrl.trim()) ||
    !tokenExchangeComplete;

  function handleSubmit() {
    if (shape === "oauth" || shape === "device-code") {
      // Sub-flow components own placeholder creation so they can
      // short-circuit when a prior session already authorized.
      setAuthFlowActive(true);
    } else {
      // api-key, token-exchange, no-auth — all go through POST /keys.
      void submit();
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-start gap-3 rounded-md border bg-muted/30 p-3">
        {entry.icon_url ? (
          <img
            src={entry.icon_url}
            alt=""
            className="h-8 w-8 rounded"
            loading="lazy"
          />
        ) : null}
        <div className="flex flex-col gap-0.5">
          <h3 className="font-medium">{entry.name}</h3>
          {entry.description ? (
            <p className="text-xs text-muted-foreground">{entry.description}</p>
          ) : null}
          <p className="text-xs text-muted-foreground">
            Auth: <code>{entry.auth_method}</code>
          </p>
        </div>
      </div>

      <div className="flex flex-col gap-3">
        <Field label="Label" htmlFor="pair-aikey-label">
          <Input
            id="pair-aikey-label"
            value={label}
            onChange={(e) => {
              setLabel(e.target.value);
            }}
            autoFocus
          />
        </Field>

        {entry.requires_gateway_url ? (
          <Field label="Instance URL" htmlFor="pair-aikey-url">
            <Input
              id="pair-aikey-url"
              value={endpointUrl}
              onChange={(e) => {
                setEndpointUrl(e.target.value);
              }}
              placeholder="https://your-instance.example.com"
            />
          </Field>
        ) : null}

        {needsCredentialInput ? (
          <Field label="API key" htmlFor="pair-aikey-credential">
            <Input
              id="pair-aikey-credential"
              type="password"
              autoComplete="off"
              value={credential}
              onChange={(e) => {
                setCredential(e.target.value);
              }}
              placeholder="sk-..."
            />
            {entry.api_key_url ? (
              <a
                href={entry.api_key_url}
                target="_blank"
                rel="noopener noreferrer"
                className="mt-1 inline-flex items-center gap-1 text-xs text-muted-foreground underline-offset-2 hover:underline"
              >
                Get an API key
                <ExternalLink className="h-3 w-3" />
              </a>
            ) : null}
          </Field>
        ) : null}

        {shape === "token-exchange"
          ? (entry.token_exchange_credential_fields ?? []).map((f) => (
              <Field
                key={f.name}
                label={f.label || f.name}
                htmlFor={`pair-aikey-tx-${f.name}`}
              >
                <Input
                  id={`pair-aikey-tx-${f.name}`}
                  type={f.secret ? "password" : "text"}
                  autoComplete="off"
                  value={tokenFields[f.name] ?? ""}
                  onChange={(e) => {
                    const val = e.target.value;
                    setTokenFields((prev) => ({ ...prev, [f.name]: val }));
                  }}
                  placeholder={f.placeholder ?? ""}
                />
              </Field>
            ))
          : null}

        {shape === "no-auth" ? (
          <p className="text-xs text-muted-foreground">
            This service doesn't need a credential. Click Connect to
            add it to your services.
          </p>
        ) : null}
      </div>

      {error ? <ErrorLine message={error} /> : null}

      <Button onClick={handleSubmit} disabled={submitDisabled}>
        {submitLabel}
      </Button>
    </div>
  );
}

function Field({
  label,
  htmlFor,
  children,
}: {
  readonly label: string;
  readonly htmlFor: string;
  readonly children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
    </div>
  );
}

function ErrorLine({ message }: { readonly message: string }) {
  return (
    <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {message}
    </p>
  );
}

