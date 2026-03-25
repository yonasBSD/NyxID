import { useState } from "react";
import { useParams, useNavigate } from "@tanstack/react-router";
import {
  useKey,
  useDeleteKey,
  useUpdateKey,
  useUpdateEndpoint,
  useUpdateExternalApiKey,
  useUpdateUserService,
} from "@/hooks/use-keys";
import { useNodes } from "@/hooks/use-nodes";
import { ApiError } from "@/lib/api-client";
import { copyToClipboard } from "@/lib/utils";
import { PageHeader } from "@/components/shared/page-header";
import { Breadcrumb } from "@/components/shared/breadcrumb";
import { SshServiceInstructions } from "@/components/dashboard/ssh-service-instructions";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Globe,
  KeyRound,
  Server,
  Router,
  Pencil,
  Trash2,
  RefreshCw,
  Check,
  X,
  Terminal,
  Copy,
  Shield,
} from "lucide-react";
import { toast } from "sonner";
import type { SshServiceConfig } from "@/types/api";

function statusVariant(
  status: string,
): "default" | "secondary" | "destructive" | "outline" {
  switch (status) {
    case "active":
      return "default";
    case "expired":
      return "secondary";
    case "revoked":
    case "refresh_failed":
      return "destructive";
    default:
      return "outline";
  }
}

function EndpointSection({
  endpointUrl,
  endpointId,
  nodeRouted,
}: {
  readonly endpointUrl: string;
  readonly endpointId: string;
  readonly nodeRouted: boolean;
}) {
  const [editing, setEditing] = useState(false);
  const [url, setUrl] = useState(endpointUrl);
  const updateEndpoint = useUpdateEndpoint();

  function handleSave() {
    if (!url.trim()) return;
    updateEndpoint.mutate(
      { endpointId, url: url.trim() },
      {
        onSuccess: () => {
          toast.success("Endpoint updated");
          setEditing(false);
        },
        onError: (err) => {
          const message =
            err instanceof ApiError ? err.message : "Failed to update endpoint";
          toast.error(message);
        },
      },
    );
  }

  function handleCancel() {
    setUrl(endpointUrl);
    setEditing(false);
  }

  const isEmpty = !endpointUrl;

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <Globe className="h-4 w-4 text-primary" />
          <CardTitle className="text-sm">Endpoint</CardTitle>
        </div>
        <CardDescription>Target URL for proxied requests</CardDescription>
      </CardHeader>
      <CardContent>
        {editing ? (
          <div className="flex items-center gap-2">
            <Input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://api.example.com/v1"
              className="flex-1 font-mono text-sm"
            />
            <Button
              size="icon"
              variant="ghost"
              onClick={handleSave}
              disabled={updateEndpoint.isPending}
            >
              <Check className="h-4 w-4" />
            </Button>
            <Button size="icon" variant="ghost" onClick={handleCancel}>
              <X className="h-4 w-4" />
            </Button>
          </div>
        ) : isEmpty && nodeRouted ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <span>Resolved by node agent</span>
            <Button
              size="icon"
              variant="ghost"
              onClick={() => setEditing(true)}
            >
              <Pencil className="h-4 w-4" />
            </Button>
          </div>
        ) : (
          <div className="flex items-center justify-between gap-2">
            <code className="truncate rounded bg-muted px-2 py-1 font-mono text-sm">
              {endpointUrl}
            </code>
            <Button
              size="icon"
              variant="ghost"
              onClick={() => setEditing(true)}
            >
              <Pencil className="h-4 w-4" />
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function ApiKeySection({
  apiKeyId,
  credentialType,
  status,
  expiresAt,
  lastUsedAt,
  errorMessage,
}: {
  readonly apiKeyId: string;
  readonly credentialType: string;
  readonly status: string;
  readonly expiresAt: string | null;
  readonly lastUsedAt: string | null;
  readonly errorMessage: string | null;
}) {
  const [rotating, setRotating] = useState(false);
  const [newCredential, setNewCredential] = useState("");
  const updateApiKey = useUpdateExternalApiKey();

  function handleRotate() {
    if (!newCredential.trim()) return;
    updateApiKey.mutate(
      { keyId: apiKeyId, credential: newCredential.trim() },
      {
        onSuccess: () => {
          toast.success("Credential rotated");
          setRotating(false);
          setNewCredential("");
        },
        onError: (err) => {
          const message =
            err instanceof ApiError
              ? err.message
              : "Failed to rotate credential";
          toast.error(message);
        },
      },
    );
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <KeyRound className="h-4 w-4 text-primary" />
          <CardTitle className="text-sm">API Key</CardTitle>
        </div>
        <CardDescription>Authentication credential</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-3">
          <Badge variant={statusVariant(status)}>{status}</Badge>
          <span className="text-xs text-muted-foreground">
            Type: {credentialType}
          </span>
        </div>

        {errorMessage && (
          <p className="text-xs text-destructive">{errorMessage}</p>
        )}

        {expiresAt && (
          <p className="text-xs text-muted-foreground">
            Expires: {new Date(expiresAt).toLocaleString()}
          </p>
        )}

        {lastUsedAt && (
          <p className="text-xs text-muted-foreground">
            Last used: {new Date(lastUsedAt).toLocaleString()}
          </p>
        )}

        {rotating ? (
          <div className="space-y-2">
            <Input
              type="password"
              value={newCredential}
              onChange={(e) => setNewCredential(e.target.value)}
              placeholder="Enter new credential"
            />
            <div className="flex gap-2">
              <Button
                size="sm"
                onClick={handleRotate}
                disabled={updateApiKey.isPending || !newCredential.trim()}
              >
                Save
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  setRotating(false);
                  setNewCredential("");
                }}
              >
                Cancel
              </Button>
            </div>
          </div>
        ) : (
          <Button size="sm" variant="outline" onClick={() => setRotating(true)}>
            <RefreshCw className="mr-2 h-3 w-3" />
            Rotate Credential
          </Button>
        )}
      </CardContent>
    </Card>
  );
}

function ServiceSection({
  slug,
  authMethod,
  authKeyName,
  isActive,
  serviceId,
}: {
  readonly slug: string;
  readonly authMethod: string;
  readonly authKeyName: string;
  readonly isActive: boolean;
  readonly serviceId: string;
}) {
  const updateService = useUpdateUserService();

  function toggleActive() {
    updateService.mutate(
      { serviceId, is_active: !isActive },
      {
        onSuccess: () => {
          toast.success(isActive ? "Service deactivated" : "Service activated");
        },
        onError: (err) => {
          const message =
            err instanceof ApiError ? err.message : "Failed to update service";
          toast.error(message);
        },
      },
    );
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <Server className="h-4 w-4 text-primary" />
          <CardTitle className="text-sm">Service</CardTitle>
        </div>
        <CardDescription>Proxy routing configuration</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-3">
          <code className="rounded bg-muted px-2 py-1 font-mono text-sm">
            /proxy/s/{slug}
          </code>
          <Badge variant={isActive ? "default" : "secondary"}>
            {isActive ? "Active" : "Inactive"}
          </Badge>
        </div>

        <div className="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
          <div>
            <span className="font-medium text-foreground">Auth method:</span>{" "}
            {authMethod}
          </div>
          <div>
            <span className="font-medium text-foreground">Auth key:</span>{" "}
            {authKeyName}
          </div>
        </div>

        <Button
          size="sm"
          variant="outline"
          onClick={toggleActive}
          disabled={updateService.isPending}
        >
          {isActive ? "Deactivate" : "Activate"}
        </Button>
      </CardContent>
    </Card>
  );
}

function RoutingSection({
  nodeId,
  serviceId,
}: {
  readonly nodeId: string | null;
  readonly serviceId: string;
}) {
  const [picking, setPicking] = useState(false);
  const { data: nodes } = useNodes();
  const updateService = useUpdateUserService();

  function handleSelectNode(selectedNodeId: string) {
    const id = selectedNodeId === "direct" ? "" : selectedNodeId;
    updateService.mutate(
      { serviceId, node_id: id },
      {
        onSuccess: () => {
          toast.success(id ? "Route updated" : "Switched to direct routing");
          setPicking(false);
        },
        onError: (err) => {
          const message =
            err instanceof ApiError ? err.message : "Failed to update routing";
          toast.error(message);
        },
      },
    );
  }

  const allNodes = nodes ?? [];
  const currentNodeName = nodeId
    ? (nodes?.find((n) => n.id === nodeId)?.name ?? nodeId)
    : null;

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <Router className="h-4 w-4 text-primary" />
          <CardTitle className="text-sm">Routing</CardTitle>
        </div>
        <CardDescription>How requests reach the endpoint</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-2">
          <Badge variant={nodeId ? "default" : "outline"}>
            {nodeId ? `Via node: ${currentNodeName}` : "Direct"}
          </Badge>
        </div>

        {picking ? (
          <div className="space-y-2">
            <Label className="text-xs">Select routing</Label>
            <Select
              onValueChange={handleSelectNode}
              defaultValue={nodeId ?? "direct"}
            >
              <SelectTrigger>
                <SelectValue placeholder="Select routing" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="direct">Direct (no node)</SelectItem>
                {allNodes.map((n) => (
                  <SelectItem
                    key={n.id}
                    value={n.id}
                    disabled={n.status !== "online"}
                  >
                    {n.name} ({n.status})
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {allNodes.length === 0 && (
              <p className="text-xs text-muted-foreground">
                No nodes registered. Register a node first.
              </p>
            )}
            <Button
              size="sm"
              variant="outline"
              onClick={() => setPicking(false)}
            >
              Cancel
            </Button>
          </div>
        ) : (
          <Button size="sm" variant="outline" onClick={() => setPicking(true)}>
            {nodeId ? "Change Route" : "Route via Node"}
          </Button>
        )}
      </CardContent>
    </Card>
  );
}

function NodeSetupHelper({
  slug,
  endpointUrl,
  authMethod,
  authKeyName,
  catalogServiceName,
}: {
  readonly slug: string;
  readonly endpointUrl: string;
  readonly authMethod: string;
  readonly authKeyName: string;
  readonly catalogServiceName: string | null;
}) {
  const urlFlag = endpointUrl ? ` \\\n  --url ${endpointUrl}` : "";

  let credentialFlags: string;
  switch (authMethod) {
    case "bearer":
      credentialFlags = ` \\\n  --header ${authKeyName} \\\n  --secret-format bearer`;
      break;
    case "header":
      credentialFlags = ` \\\n  --header ${authKeyName}`;
      break;
    case "query":
      credentialFlags = ` \\\n  --query-param ${authKeyName}`;
      break;
    case "basic":
      credentialFlags = ` \\\n  --header ${authKeyName} \\\n  --secret-format basic`;
      break;
    case "none":
      credentialFlags = "";
      break;
    default:
      credentialFlags = ` \\\n  --header ${authKeyName}`;
  }

  const setupCommand = `nyxid-node credentials setup --service ${slug}`;
  const manualCommand = `nyxid-node credentials add \\\n  --service ${slug}${urlFlag}${credentialFlags}`;

  function handleCopySetup() {
    void copyToClipboard(setupCommand).then(() => {
      toast.success("Command copied to clipboard");
    });
  }

  function handleCopyManual() {
    void copyToClipboard(manualCommand).then(() => {
      toast.success("Command copied to clipboard");
    });
  }

  return (
    <Card className="md:col-span-2">
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <Terminal className="h-4 w-4 text-primary" />
          <CardTitle className="text-sm">Node Setup</CardTitle>
        </div>
        <CardDescription>
          Run this on your node to configure credentials
          {catalogServiceName ? ` for ${catalogServiceName}` : ""}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-[11px] font-medium text-muted-foreground">
          Recommended (auto-detects requirements):
        </p>
        <div className="relative">
          <pre className="overflow-x-auto rounded-lg bg-muted p-3 font-mono text-xs leading-relaxed">
            {setupCommand}
          </pre>
          <Button
            size="icon"
            variant="ghost"
            className="absolute right-2 top-2 h-7 w-7"
            onClick={handleCopySetup}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
        </div>
        <p className="text-[11px] font-medium text-muted-foreground">Manual:</p>
        <div className="relative">
          <pre className="overflow-x-auto rounded-lg bg-muted p-3 font-mono text-xs leading-relaxed">
            {manualCommand}
          </pre>
          <Button
            size="icon"
            variant="ghost"
            className="absolute right-2 top-2 h-7 w-7"
            onClick={handleCopyManual}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
        </div>
        <p className="text-[11px] text-muted-foreground">
          The agent will prompt for the secret value securely. After adding, the
          credential will be encrypted and stored locally on the node.
        </p>
      </CardContent>
    </Card>
  );
}

function SshConnectionSection({
  sshHost,
  sshPort,
  caPublicKey,
  principals,
  certTtlMinutes,
}: {
  readonly sshHost: string;
  readonly sshPort: number;
  readonly caPublicKey: string | null;
  readonly principals: readonly string[] | null;
  readonly certTtlMinutes: number | null;
}) {
  function handleCopyCa() {
    if (!caPublicKey) return;
    void copyToClipboard(caPublicKey).then(() => {
      toast.success("CA public key copied");
    });
  }

  return (
    <Card className="md:col-span-2">
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <Shield className="h-4 w-4 text-primary" />
          <CardTitle className="text-sm">SSH Connection</CardTitle>
        </div>
        <CardDescription>
          SSH certificate authentication details
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div>
            <span className="text-xs font-medium text-muted-foreground">
              Host
            </span>
            <p className="font-mono">
              {sshHost}:{sshPort}
            </p>
          </div>
          {certTtlMinutes !== null && (
            <div>
              <span className="text-xs font-medium text-muted-foreground">
                Certificate TTL
              </span>
              <p>{certTtlMinutes} minutes</p>
            </div>
          )}
        </div>

        {principals && principals.length > 0 && (
          <div>
            <span className="text-xs font-medium text-muted-foreground">
              Allowed Principals
            </span>
            <div className="mt-1 flex flex-wrap gap-1.5">
              {principals.map((p) => (
                <Badge key={p} variant="secondary" className="text-xs">
                  {p}
                </Badge>
              ))}
            </div>
          </div>
        )}

        {caPublicKey && (
          <div>
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-muted-foreground">
                CA Public Key
              </span>
              <Button
                size="sm"
                variant="ghost"
                onClick={handleCopyCa}
                className="h-6 px-2"
              >
                <Copy className="mr-1 h-3 w-3" />
                <span className="text-xs">Copy</span>
              </Button>
            </div>
            <pre className="mt-1 overflow-x-auto rounded-lg bg-muted p-3 font-mono text-xs leading-relaxed">
              {caPublicKey}
            </pre>
          </div>
        )}

        <div className="rounded-lg border border-border bg-muted/50 p-3 space-y-2">
          <p className="text-xs font-medium">Target Machine Setup</p>
          <ol className="list-decimal list-inside space-y-1 text-xs text-muted-foreground">
            <li>
              Add the CA public key to{" "}
              <code className="rounded bg-background px-1">
                /etc/ssh/trusted-user-ca-keys.pem
              </code>
            </li>
            <li>
              Add{" "}
              <code className="rounded bg-background px-1">
                TrustedUserCAKeys /etc/ssh/trusted-user-ca-keys.pem
              </code>{" "}
              to{" "}
              <code className="rounded bg-background px-1">
                /etc/ssh/sshd_config
              </code>
            </li>
            <li>Restart sshd</li>
          </ol>
        </div>
      </CardContent>
    </Card>
  );
}

function DeleteKeyDialog({
  open,
  onOpenChange,
  keyId,
}: {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly keyId: string;
}) {
  const navigate = useNavigate();
  const deleteKey = useDeleteKey();

  function handleDelete() {
    deleteKey.mutate(keyId, {
      onSuccess: () => {
        toast.success("Key deleted");
        void navigate({ to: "/keys", search: {} });
      },
      onError: (err) => {
        const message =
          err instanceof ApiError ? err.message : "Failed to delete key";
        toast.error(message);
      },
    });
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete Service</DialogTitle>
          <DialogDescription>
            This will deactivate the service and revoke the API key. Proxied
            requests using this key will stop working. This action cannot be
            undone.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={handleDelete}
            disabled={deleteKey.isPending}
          >
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function LabelEditor({
  keyId,
  currentLabel,
}: {
  readonly keyId: string;
  readonly currentLabel: string;
}) {
  const [editing, setEditing] = useState(false);
  const [label, setLabel] = useState(currentLabel);
  const updateKey = useUpdateKey();

  function handleSave() {
    const trimmed = label.trim();
    if (!trimmed || trimmed === currentLabel) {
      setLabel(currentLabel);
      setEditing(false);
      return;
    }
    updateKey.mutate(
      { keyId, label: trimmed },
      {
        onSuccess: () => {
          toast.success("Label updated");
          setEditing(false);
        },
        onError: (err) => {
          const message =
            err instanceof ApiError ? err.message : "Failed to update label";
          toast.error(message);
        },
      },
    );
  }

  function handleCancel() {
    setLabel(currentLabel);
    setEditing(false);
  }

  if (editing) {
    return (
      <div className="flex items-center gap-2">
        <Input
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          className="font-display text-2xl font-normal tracking-tight md:text-4xl"
          autoFocus
          onKeyDown={(e) => {
            if (e.key === "Enter") handleSave();
            if (e.key === "Escape") handleCancel();
          }}
        />
        <Button
          size="icon"
          variant="ghost"
          onClick={handleSave}
          disabled={updateKey.isPending}
        >
          <Check className="h-4 w-4" />
        </Button>
        <Button size="icon" variant="ghost" onClick={handleCancel}>
          <X className="h-4 w-4" />
        </Button>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2">
      <h2 className="font-display text-3xl font-normal tracking-tight md:text-5xl">
        {currentLabel}
      </h2>
      <Button
        size="icon"
        variant="ghost"
        onClick={() => setEditing(true)}
        className="h-8 w-8"
      >
        <Pencil className="h-4 w-4" />
      </Button>
    </div>
  );
}

export function KeyDetailPage() {
  const { keyId } = useParams({ strict: false }) as { keyId: string };
  const navigate = useNavigate();
  const { data: keyInfo, isLoading, error } = useKey(keyId);
  const [deleteOpen, setDeleteOpen] = useState(false);

  if (isLoading) {
    return (
      <div className="space-y-8">
        <Skeleton className="h-20 w-full" />
        <div className="grid gap-4 md:grid-cols-2">
          {Array.from({ length: 4 }, (_, i) => (
            <Skeleton key={i} className="h-48" />
          ))}
        </div>
      </div>
    );
  }

  if (error || !keyInfo) {
    return (
      <div className="space-y-8">
        <PageHeader
          title="Key Not Found"
          breadcrumbs={[
            { label: "AI Services", to: "/keys" },
            { label: "Not Found" },
          ]}
        />
        <Card>
          <CardContent className="py-8 text-center text-sm text-destructive">
            {error instanceof ApiError
              ? error.message
              : "Failed to load key details."}
          </CardContent>
        </Card>
      </div>
    );
  }

  const isSsh = keyInfo.service_type === "ssh";
  const hasCertAuth = isSsh && keyInfo.ssh_ca_public_key !== null;
  const sshServiceId = keyInfo.catalog_service_id;

  const sshConfig: SshServiceConfig | null =
    isSsh && keyInfo.ssh_host && keyInfo.ssh_port !== null
      ? {
          host: keyInfo.ssh_host,
          port: keyInfo.ssh_port,
          certificate_auth_enabled: hasCertAuth,
          certificate_ttl_minutes: keyInfo.ssh_certificate_ttl_minutes ?? 30,
          allowed_principals: keyInfo.ssh_allowed_principals
            ? [...keyInfo.ssh_allowed_principals]
            : [],
          ca_public_key: keyInfo.ssh_ca_public_key,
        }
      : null;

  return (
    <div className="space-y-8">
      <div className="flex flex-col gap-2">
        <Breadcrumb
          items={[
            { label: "AI Services", to: "/keys" },
            { label: keyInfo.label },
          ]}
        />
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex flex-col gap-2">
            <LabelEditor keyId={keyInfo.id} currentLabel={keyInfo.label} />
            <p className="text-sm text-muted-foreground">
              {keyInfo.catalog_service_name
                ? `${keyInfo.catalog_service_name} -- /proxy/s/${keyInfo.slug}`
                : `/proxy/s/${keyInfo.slug}`}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {hasCertAuth && sshServiceId && (
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  void navigate({
                    to: "/ssh/$serviceId/terminal",
                    params: { serviceId: sshServiceId },
                    search: {
                      principal: keyInfo.ssh_allowed_principals?.[0],
                      returnKeyId: keyInfo.id,
                    },
                  })
                }
              >
                <Terminal className="mr-2 h-4 w-4" />
                Terminal
              </Button>
            )}
            <Button
              variant="destructive"
              size="sm"
              onClick={() => setDeleteOpen(true)}
            >
              <Trash2 className="mr-2 h-4 w-4" />
              Delete
            </Button>
          </div>
        </div>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <EndpointSection
          endpointUrl={keyInfo.endpoint_url}
          endpointId={keyInfo.endpoint_id}
          nodeRouted={keyInfo.node_id !== null}
        />

        {keyInfo.service_type === "ssh" &&
        keyInfo.ssh_host &&
        keyInfo.ssh_port !== null ? (
          <SshConnectionSection
            sshHost={keyInfo.ssh_host}
            sshPort={keyInfo.ssh_port}
            caPublicKey={keyInfo.ssh_ca_public_key}
            principals={keyInfo.ssh_allowed_principals}
            certTtlMinutes={keyInfo.ssh_certificate_ttl_minutes}
          />
        ) : (
          <ApiKeySection
            apiKeyId={keyInfo.api_key_id}
            credentialType={keyInfo.credential_type}
            status={keyInfo.status}
            expiresAt={keyInfo.expires_at}
            lastUsedAt={keyInfo.last_used_at}
            errorMessage={keyInfo.error_message}
          />
        )}

        <ServiceSection
          slug={keyInfo.slug}
          authMethod={keyInfo.auth_method}
          authKeyName={keyInfo.auth_key_name}
          isActive={keyInfo.is_active}
          serviceId={keyInfo.id}
        />

        <RoutingSection nodeId={keyInfo.node_id} serviceId={keyInfo.id} />

        {keyInfo.node_id && !isSsh && (
          <NodeSetupHelper
            slug={keyInfo.slug}
            endpointUrl={keyInfo.endpoint_url}
            authMethod={keyInfo.auth_method}
            authKeyName={keyInfo.auth_key_name}
            catalogServiceName={keyInfo.catalog_service_name}
          />
        )}
      </div>

      {sshConfig && sshServiceId && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Connection Instructions</CardTitle>
            <CardDescription>
              How to connect to this SSH service
            </CardDescription>
          </CardHeader>
          <CardContent>
            <SshServiceInstructions
              serviceId={sshServiceId}
              serviceSlug={keyInfo.slug}
              sshConfig={sshConfig}
            />
          </CardContent>
        </Card>
      )}

      <DeleteKeyDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        keyId={keyInfo.id}
      />
    </div>
  );
}
