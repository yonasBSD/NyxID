// Per-kind wizard panels. Each panel owns the "confirm action" step
// (review prefill, maybe tweak a field, click "Do it"), then hands a
// DisplayOnce-ready secret back to the parent page via `onSecret`.
//
// The parent fires the `/cli-pairings/{id}/complete` POST with the
// ack payload when the user finishes DisplayOnce — panels don't
// touch that endpoint.

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api } from "@/lib/api-client";
import { reservePairingAction, withRewindOnError } from "./reserve-action";
import type {
  ApiKeyCreatePrefill,
  NodeRegisterPrefill,
  RotatePrefill,
} from "./types";

// ── api-key-create panel ────────────────────────────────────────────

interface ApiKeyCreateResponse {
  readonly id: string;
  readonly full_key: string;
}

export interface ApiKeyCreateSuccess {
  readonly kind: "api-key-create";
  readonly api_key_id: string;
  readonly full_key: string;
}

interface ApiKeyCreateConfirmProps {
  readonly prefill: ApiKeyCreatePrefill;
  readonly pairingId: string;
  readonly onSuccess: (result: ApiKeyCreateSuccess) => void;
}

export function ApiKeyCreateConfirm({
  prefill,
  pairingId,
  onSuccess,
}: ApiKeyCreateConfirmProps) {
  const [name, setName] = useState(prefill.name ?? "");
  const [platform, setPlatform] = useState(prefill.platform ?? "");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setLoading(true);
    setError(null);
    try {
      const body: Record<string, unknown> = {
        name,
        scopes: prefill.scopes ?? "read write",
        allow_all_services: prefill.allow_all_services ?? true,
        allow_all_nodes: prefill.allow_all_nodes ?? true,
      };
      if (platform) body.platform = platform;
      if (prefill.callback_url) body.callback_url = prefill.callback_url;
      if (prefill.org_id) body.target_org_id = prefill.org_id;
      if (prefill.allowed_services_csv) {
        body.allowed_service_ids = prefill.allowed_services_csv
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean);
        body.allow_all_services = false;
      }
      if (prefill.allowed_nodes_csv) {
        body.allowed_node_ids = prefill.allowed_nodes_csv
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean);
        body.allow_all_nodes = false;
      }
      // Match the terminal path's convention (`cli/src/commands/api_key.rs`):
      // a positive `expires_in_days` becomes an RFC-3339 `expires_at`; a
      // value of `0` means "no expiry" and is omitted so the backend
      // stores null (not an already-expired key).
      if (prefill.expires_in_days != null && prefill.expires_in_days > 0) {
        const expiry = new Date();
        expiry.setDate(expiry.getDate() + prefill.expires_in_days);
        body.expires_at = expiry.toISOString();
      }

      await reservePairingAction(pairingId);
      const res = await withRewindOnError(pairingId, () =>
        api.post<ApiKeyCreateResponse>("/api-keys", body),
      );
      onSuccess({
        kind: "api-key-create",
        api_key_id: res.id,
        full_key: res.full_key,
      });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h2 className="font-display text-xl font-semibold">
          Create an API key
        </h2>
        <p className="text-sm text-muted-foreground">
          Review the details your CLI sent and confirm to mint the key.
        </p>
      </div>
      <div className="flex flex-col gap-3">
        <Field label="Name" htmlFor="pair-api-key-name">
          <Input
            id="pair-api-key-name"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            autoFocus
          />
        </Field>
        <Field label="Platform" htmlFor="pair-api-key-platform">
          <Input
            id="pair-api-key-platform"
            value={platform}
            placeholder="claude-code, codex, ..."
            onChange={(e) => {
              setPlatform(e.target.value);
            }}
          />
        </Field>
      </div>
      {error ? <ErrorLine message={error} /> : null}
      <Button
        onClick={() => void submit()}
        disabled={loading || !name.trim()}
      >
        {loading ? "Creating..." : "Create key"}
      </Button>
    </div>
  );
}

// ── api-key-rotate panel ────────────────────────────────────────────

interface ApiKeyRotateResponse {
  readonly id: string;
  readonly full_key: string;
}

export interface ApiKeyRotateSuccess {
  readonly kind: "api-key-rotate";
  readonly resource_id: string;
  readonly full_key: string;
}

interface ApiKeyRotateConfirmProps {
  readonly prefill: RotatePrefill;
  readonly pairingId: string;
  readonly onSuccess: (result: ApiKeyRotateSuccess) => void;
}

export function ApiKeyRotateConfirm({
  prefill,
  pairingId,
  onSuccess,
}: ApiKeyRotateConfirmProps) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setLoading(true);
    setError(null);
    try {
      await reservePairingAction(pairingId);
      const res = await withRewindOnError(pairingId, () =>
        api.post<ApiKeyRotateResponse>(
          `/api-keys/${encodeURIComponent(prefill.resource_id)}/rotate`,
        ),
      );
      onSuccess({
        kind: "api-key-rotate",
        resource_id: res.id,
        full_key: res.full_key,
      });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h2 className="font-display text-xl font-semibold">
          Rotate API key
        </h2>
        <p className="text-sm text-muted-foreground">
          Rotating <strong>{prefill.display_name}</strong> will issue a
          new key and immediately revoke the previous one.
        </p>
      </div>
      {error ? <ErrorLine message={error} /> : null}
      <Button onClick={() => void submit()} disabled={loading}>
        {loading ? "Rotating..." : "Rotate key"}
      </Button>
    </div>
  );
}

// ── node-register-token panel ───────────────────────────────────────

interface NodeRegisterTokenResponse {
  readonly token_id: string;
  readonly token: string;
  readonly name: string;
}

export interface NodeRegisterSuccess {
  readonly kind: "node-register-token";
  readonly token_id: string;
  readonly token: string;
}

interface NodeRegisterConfirmProps {
  readonly prefill: NodeRegisterPrefill;
  readonly pairingId: string;
  readonly onSuccess: (result: NodeRegisterSuccess) => void;
}

/**
 * Must match the fallback used by the terminal path in
 * `cli/src/commands/node.rs::RegisterToken`. The backend requires a
 * non-empty lowercase name, so we can't send `undefined` — both
 * transports default to `my-node` when the user (or prefill) didn't
 * provide one, keeping the headless pairing flow usable for the
 * common no-flag invocation.
 */
const DEFAULT_NODE_NAME = "my-node";

export function NodeRegisterConfirm({
  prefill,
  pairingId,
  onSuccess,
}: NodeRegisterConfirmProps) {
  const [name, setName] = useState(prefill.name ?? "");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setLoading(true);
    setError(null);
    try {
      const trimmed = name.trim();
      const effectiveName = trimmed.length > 0 ? trimmed : DEFAULT_NODE_NAME;
      await reservePairingAction(pairingId);
      const res = await withRewindOnError(pairingId, () =>
        api.post<NodeRegisterTokenResponse>(
          "/nodes/register-token",
          { name: effectiveName },
        ),
      );
      onSuccess({
        kind: "node-register-token",
        token_id: res.token_id,
        token: res.token,
      });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h2 className="font-display text-xl font-semibold">
          Generate node registration token
        </h2>
        <p className="text-sm text-muted-foreground">
          Use this token with <code>nyxid node register</code> to
          connect a new node.
        </p>
      </div>
      <Field label="Node name (optional)" htmlFor="pair-node-name">
        <Input
          id="pair-node-name"
          value={name}
          placeholder={DEFAULT_NODE_NAME}
          onChange={(e) => {
            setName(e.target.value);
          }}
          autoFocus
        />
        <p className="mt-1 text-xs text-muted-foreground">
          Leave blank to use the default <code>{DEFAULT_NODE_NAME}</code>.
        </p>
      </Field>
      {error ? <ErrorLine message={error} /> : null}
      <Button onClick={() => void submit()} disabled={loading}>
        {loading ? "Generating..." : "Generate token"}
      </Button>
    </div>
  );
}

// ── node-rotate-token panel ─────────────────────────────────────────

interface NodeRotateTokenResponse {
  readonly auth_token: string;
  readonly signing_secret: string;
}

export interface NodeRotateSuccess {
  readonly kind: "node-rotate-token";
  readonly resource_id: string;
  readonly auth_token: string;
  readonly signing_secret: string;
}

interface NodeRotateConfirmProps {
  readonly prefill: RotatePrefill;
  readonly pairingId: string;
  readonly onSuccess: (result: NodeRotateSuccess) => void;
}

export function NodeRotateConfirm({
  prefill,
  pairingId,
  onSuccess,
}: NodeRotateConfirmProps) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setLoading(true);
    setError(null);
    try {
      await reservePairingAction(pairingId);
      const res = await withRewindOnError(pairingId, () =>
        api.post<NodeRotateTokenResponse>(
          `/nodes/${encodeURIComponent(prefill.resource_id)}/rotate-token`,
        ),
      );
      onSuccess({
        kind: "node-rotate-token",
        resource_id: prefill.resource_id,
        auth_token: res.auth_token,
        signing_secret: res.signing_secret,
      });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h2 className="font-display text-xl font-semibold">
          Rotate node token
        </h2>
        <p className="text-sm text-muted-foreground">
          Rotating <strong>{prefill.display_name}</strong> issues a new
          auth token + signing secret and revokes the previous pair.
        </p>
      </div>
      {error ? <ErrorLine message={error} /> : null}
      <Button onClick={() => void submit()} disabled={loading}>
        {loading ? "Rotating..." : "Rotate token"}
      </Button>
    </div>
  );
}

// ── helpers ─────────────────────────────────────────────────────────

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

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return "Something went wrong. Please try again.";
}
