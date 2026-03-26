import { useState } from "react";
import { useParams, useNavigate } from "@tanstack/react-router";
import {
  useApiKey,
  useDeleteApiKey,
  useRotateApiKey,
  useUpdateApiKey,
} from "@/hooks/use-api-keys";
import { useKeys } from "@/hooks/use-keys";
import { useNodes } from "@/hooks/use-nodes";
import { ApiError } from "@/lib/api-client";
import {
  maskApiKey,
  formatDate,
  formatRelativeTime,
  copyToClipboard,
} from "@/lib/utils";
import { PageHeader } from "@/components/shared/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  KeyRound,
  Shield,
  HardDrive,
  Pencil,
  Trash2,
  RefreshCw,
  Check,
  X,
  Copy,
} from "lucide-react";
import { toast } from "sonner";

const AVAILABLE_SCOPES = ["proxy", "read", "write", "admin"];

function DetailsCard({
  name,
  description,
  keyPrefix,
  scopes,
  isActive,
  createdAt,
  lastUsedAt,
  expiresAt,
  keyId,
}: {
  readonly name: string;
  readonly description: string | null;
  readonly keyPrefix: string;
  readonly scopes: string;
  readonly isActive: boolean;
  readonly createdAt: string;
  readonly lastUsedAt: string | null;
  readonly expiresAt: string | null;
  readonly keyId: string;
}) {
  const [editingName, setEditingName] = useState(false);
  const [editName, setEditName] = useState(name);
  const [editDesc, setEditDesc] = useState(description ?? "");
  const [editingScopes, setEditingScopes] = useState(false);
  const [editScopes, setEditScopes] = useState<readonly string[]>(() =>
    scopes.trim().split(/\s+/).filter(Boolean),
  );
  const updateApiKey = useUpdateApiKey();

  function handleSaveName() {
    if (!editName.trim()) return;
    updateApiKey.mutate(
      {
        keyId,
        name: editName.trim(),
        description: editDesc.trim() || undefined,
      },
      {
        onSuccess: () => {
          toast.success("Key updated");
          setEditingName(false);
        },
        onError: (err) => {
          const message =
            err instanceof ApiError ? err.message : "Failed to update";
          toast.error(message);
        },
      },
    );
  }

  const scopesList = scopes.trim().split(/\s+/).filter(Boolean);

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <KeyRound className="h-4 w-4 text-primary" />
          <CardTitle className="text-sm">Key Details</CardTitle>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {editingName ? (
          <div className="space-y-2">
            <div className="space-y-1.5">
              <Label>Name</Label>
              <Input
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Description</Label>
              <Input
                value={editDesc}
                onChange={(e) => setEditDesc(e.target.value)}
                placeholder="Optional description"
              />
            </div>
            <div className="flex gap-2">
              <Button
                size="icon"
                variant="ghost"
                onClick={handleSaveName}
                disabled={updateApiKey.isPending}
              >
                <Check className="h-4 w-4" />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                onClick={() => {
                  setEditName(name);
                  setEditDesc(description ?? "");
                  setEditingName(false);
                }}
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
          </div>
        ) : (
          <div className="flex items-start justify-between">
            <div>
              <p className="text-sm font-medium">{name}</p>
              {description && (
                <p className="text-xs text-muted-foreground">{description}</p>
              )}
            </div>
            <Button
              size="icon"
              variant="ghost"
              onClick={() => setEditingName(true)}
            >
              <Pencil className="h-4 w-4" />
            </Button>
          </div>
        )}

        <div className="grid grid-cols-2 gap-2 text-xs">
          <div>
            <span className="text-muted-foreground">Prefix: </span>
            <code className="rounded bg-muted px-1.5 py-0.5 font-mono">
              {maskApiKey(keyPrefix)}
            </code>
          </div>
          <div>
            <span className="text-muted-foreground">Status: </span>
            <Badge
              variant={isActive ? "default" : "secondary"}
              className="text-[10px]"
            >
              {isActive ? "Active" : "Inactive"}
            </Badge>
          </div>
          <div>
            <span className="text-muted-foreground">Created: </span>
            {formatDate(createdAt)}
          </div>
          <div>
            <span className="text-muted-foreground">Last used: </span>
            {lastUsedAt ? formatRelativeTime(lastUsedAt) : "Never"}
          </div>
          {expiresAt && (
            <div className="col-span-2">
              <span className="text-muted-foreground">Expires: </span>
              {new Date(expiresAt).toLocaleString()}
            </div>
          )}
        </div>

        <div>
          <div className="flex items-center justify-between">
            <span className="text-xs text-muted-foreground">Scopes: </span>
            {!editingScopes && (
              <Button
                size="icon"
                variant="ghost"
                className="h-6 w-6"
                onClick={() => {
                  setEditScopes(scopesList);
                  setEditingScopes(true);
                }}
              >
                <Pencil className="h-3 w-3" />
              </Button>
            )}
          </div>
          {editingScopes ? (
            <div className="mt-1 space-y-2">
              <div className="flex flex-wrap gap-2">
                {AVAILABLE_SCOPES.map((s) => (
                  <div key={s} className="flex items-center gap-1.5">
                    <Checkbox
                      id={`scope-${s}`}
                      checked={editScopes.includes(s)}
                      onCheckedChange={(checked) =>
                        setEditScopes((prev) =>
                          checked ? [...prev, s] : prev.filter((p) => p !== s),
                        )
                      }
                    />
                    <Label htmlFor={`scope-${s}`} className="text-xs">
                      {s}
                    </Label>
                  </div>
                ))}
              </div>
              <div className="flex gap-2">
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-6 w-6"
                  disabled={updateApiKey.isPending}
                  onClick={() => {
                    updateApiKey.mutate(
                      { keyId, scopes: editScopes.join(" ") },
                      {
                        onSuccess: () => {
                          toast.success("Scopes updated");
                          setEditingScopes(false);
                        },
                        onError: (err) => {
                          const message =
                            err instanceof ApiError
                              ? err.message
                              : "Failed to update";
                          toast.error(message);
                        },
                      },
                    );
                  }}
                >
                  <Check className="h-4 w-4" />
                </Button>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-6 w-6"
                  onClick={() => {
                    setEditScopes(scopesList);
                    setEditingScopes(false);
                  }}
                >
                  <X className="h-4 w-4" />
                </Button>
              </div>
            </div>
          ) : (
            <div className="mt-1 flex flex-wrap gap-1">
              {scopesList.map((s) => (
                <Badge key={s} variant="secondary" className="text-xs">
                  {s}
                </Badge>
              ))}
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

function ServiceScopeCard({
  keyId,
  allowAllServices,
  allowedServiceIds,
  allowedServices,
}: {
  readonly keyId: string;
  readonly allowAllServices: boolean;
  readonly allowedServiceIds: readonly string[];
  readonly allowedServices: readonly {
    readonly id: string;
    readonly slug: string;
    readonly label: string;
    readonly catalog_service_name: string | null;
  }[];
}) {
  const [editing, setEditing] = useState(false);
  const [allowAll, setAllowAll] = useState(allowAllServices);
  const [selectedIds, setSelectedIds] =
    useState<readonly string[]>(allowedServiceIds);
  const { data: allKeys } = useKeys();
  const updateApiKey = useUpdateApiKey();

  function handleSave() {
    updateApiKey.mutate(
      {
        keyId,
        allow_all_services: allowAll,
        allowed_service_ids: allowAll ? [] : [...selectedIds],
      },
      {
        onSuccess: () => {
          toast.success("Service scope updated");
          setEditing(false);
        },
        onError: (err) => {
          const message =
            err instanceof ApiError ? err.message : "Failed to update scope";
          toast.error(message);
        },
      },
    );
  }

  function handleCancel() {
    setAllowAll(allowAllServices);
    setSelectedIds(allowedServiceIds);
    setEditing(false);
  }

  function toggleService(serviceId: string) {
    setSelectedIds((prev) =>
      prev.includes(serviceId)
        ? prev.filter((id) => id !== serviceId)
        : [...prev, serviceId],
    );
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <Shield className="h-4 w-4 text-primary" />
          <CardTitle className="text-sm">Service Scope</CardTitle>
        </div>
        <CardDescription>
          Which external services this key can access via proxy
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {editing ? (
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Checkbox
                id="allow-all-services"
                checked={allowAll}
                onCheckedChange={(checked) => setAllowAll(checked === true)}
              />
              <Label htmlFor="allow-all-services" className="text-sm">
                Allow all services
              </Label>
            </div>

            {!allowAll && (
              <div className="space-y-2 rounded-lg border border-border p-3">
                <p className="text-xs text-muted-foreground">
                  Select allowed services:
                </p>
                {allKeys && allKeys.length > 0 ? (
                  allKeys.map((k) => (
                    <div key={k.id} className="flex items-center gap-2">
                      <Checkbox
                        id={`svc-${k.id}`}
                        checked={selectedIds.includes(k.id)}
                        onCheckedChange={() => toggleService(k.id)}
                      />
                      <Label htmlFor={`svc-${k.id}`} className="text-xs">
                        {k.label}
                        <span className="text-muted-foreground">
                          {" "}
                          ({k.slug})
                        </span>
                      </Label>
                    </div>
                  ))
                ) : (
                  <p className="text-xs text-muted-foreground">
                    No external services configured yet.
                  </p>
                )}
              </div>
            )}

            <div className="flex gap-2">
              <Button
                size="sm"
                onClick={handleSave}
                disabled={updateApiKey.isPending}
              >
                Save
              </Button>
              <Button size="sm" variant="outline" onClick={handleCancel}>
                Cancel
              </Button>
            </div>
          </div>
        ) : (
          <div className="space-y-2">
            {allowAllServices ? (
              <Badge variant="outline">All services</Badge>
            ) : allowedServices.length > 0 ? (
              <div className="flex flex-wrap gap-1">
                {allowedServices.map((s) => (
                  <Badge key={s.id} variant="secondary" className="text-xs">
                    {s.label} ({s.slug})
                  </Badge>
                ))}
              </div>
            ) : (
              <Badge variant="destructive">No services (auth-only)</Badge>
            )}
            <div>
              <Button
                size="sm"
                variant="outline"
                onClick={() => setEditing(true)}
              >
                <Pencil className="mr-2 h-3 w-3" />
                Edit Scope
              </Button>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function NodeScopeCard({
  keyId,
  allowAllNodes,
  allowedNodeIds,
  allowedNodes,
}: {
  readonly keyId: string;
  readonly allowAllNodes: boolean;
  readonly allowedNodeIds: readonly string[];
  readonly allowedNodes: readonly {
    readonly id: string;
    readonly name: string;
    readonly status: string;
  }[];
}) {
  const [editing, setEditing] = useState(false);
  const [allowAll, setAllowAll] = useState(allowAllNodes);
  const [selectedIds, setSelectedIds] =
    useState<readonly string[]>(allowedNodeIds);
  const { data: allNodes } = useNodes();
  const updateApiKey = useUpdateApiKey();

  function handleSave() {
    updateApiKey.mutate(
      {
        keyId,
        allow_all_nodes: allowAll,
        allowed_node_ids: allowAll ? [] : [...selectedIds],
      },
      {
        onSuccess: () => {
          toast.success("Node scope updated");
          setEditing(false);
        },
        onError: (err) => {
          const message =
            err instanceof ApiError ? err.message : "Failed to update scope";
          toast.error(message);
        },
      },
    );
  }

  function handleCancel() {
    setAllowAll(allowAllNodes);
    setSelectedIds(allowedNodeIds);
    setEditing(false);
  }

  function toggleNode(nodeId: string) {
    setSelectedIds((prev) =>
      prev.includes(nodeId)
        ? prev.filter((id) => id !== nodeId)
        : [...prev, nodeId],
    );
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <HardDrive className="h-4 w-4 text-primary" />
          <CardTitle className="text-sm">Node Scope</CardTitle>
        </div>
        <CardDescription>
          Which nodes this key can route through
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {editing ? (
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Checkbox
                id="allow-all-nodes"
                checked={allowAll}
                onCheckedChange={(checked) => setAllowAll(checked === true)}
              />
              <Label htmlFor="allow-all-nodes" className="text-sm">
                Allow all nodes
              </Label>
            </div>

            {!allowAll && (
              <div className="space-y-2 rounded-lg border border-border p-3">
                <p className="text-xs text-muted-foreground">
                  Select allowed nodes:
                </p>
                {allNodes && allNodes.length > 0 ? (
                  allNodes.map((n) => (
                    <div key={n.id} className="flex items-center gap-2">
                      <Checkbox
                        id={`node-${n.id}`}
                        checked={selectedIds.includes(n.id)}
                        onCheckedChange={() => toggleNode(n.id)}
                      />
                      <Label htmlFor={`node-${n.id}`} className="text-xs">
                        {n.name}
                        <Badge
                          variant={
                            n.status === "online" ? "default" : "secondary"
                          }
                          className="ml-1 text-[10px]"
                        >
                          {n.status}
                        </Badge>
                      </Label>
                    </div>
                  ))
                ) : (
                  <p className="text-xs text-muted-foreground">
                    No nodes registered yet.
                  </p>
                )}
              </div>
            )}

            <div className="flex gap-2">
              <Button
                size="sm"
                onClick={handleSave}
                disabled={updateApiKey.isPending}
              >
                Save
              </Button>
              <Button size="sm" variant="outline" onClick={handleCancel}>
                Cancel
              </Button>
            </div>
          </div>
        ) : (
          <div className="space-y-2">
            {allowAllNodes ? (
              <Badge variant="outline">All nodes</Badge>
            ) : allowedNodes.length > 0 ? (
              <div className="flex flex-wrap gap-1">
                {allowedNodes.map((n) => (
                  <Badge key={n.id} variant="secondary" className="text-xs">
                    {n.name}
                  </Badge>
                ))}
              </div>
            ) : (
              <Badge variant="outline">Direct only (no nodes)</Badge>
            )}
            <div>
              <Button
                size="sm"
                variant="outline"
                onClick={() => setEditing(true)}
              >
                <Pencil className="mr-2 h-3 w-3" />
                Edit Scope
              </Button>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function RotateKeyDialog({
  open,
  onOpenChange,
  keyId,
}: {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly keyId: string;
}) {
  const rotateMutation = useRotateApiKey();
  const [newKeyValue, setNewKeyValue] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  async function handleRotate() {
    try {
      const result = await rotateMutation.mutateAsync(keyId);
      setNewKeyValue(result.full_key);
    } catch {
      toast.error("Failed to rotate key");
    }
  }

  async function handleCopy() {
    if (!newKeyValue) return;
    try {
      await copyToClipboard(newKeyValue);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error("Failed to copy");
    }
  }

  function handleClose() {
    setNewKeyValue(null);
    setCopied(false);
    onOpenChange(false);
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) handleClose();
      }}
    >
      <DialogContent>
        {newKeyValue ? (
          <>
            <DialogHeader>
              <DialogTitle>New API Key</DialogTitle>
              <DialogDescription>
                Copy your new API key now. You will not be able to see it again.
              </DialogDescription>
            </DialogHeader>
            <div className="flex items-center gap-2">
              <code className="flex-1 rounded-md bg-muted p-3 font-mono text-sm break-all select-all">
                {newKeyValue}
              </code>
              <Button
                variant="outline"
                size="icon"
                onClick={() => void handleCopy()}
              >
                {copied ? (
                  <Check className="h-4 w-4 text-success" />
                ) : (
                  <Copy className="h-4 w-4" />
                )}
              </Button>
            </div>
            <DialogFooter>
              <Button onClick={handleClose}>Done</Button>
            </DialogFooter>
          </>
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Rotate API Key</DialogTitle>
              <DialogDescription>
                This will generate a new key and invalidate the old one. Any
                applications using the current key will stop working.
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="outline" onClick={handleClose}>
                Cancel
              </Button>
              <Button
                onClick={() => void handleRotate()}
                disabled={rotateMutation.isPending}
              >
                Rotate Key
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

function DeleteKeyDialog({
  open,
  onOpenChange,
  keyId,
  keyName,
}: {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly keyId: string;
  readonly keyName: string;
}) {
  const navigate = useNavigate();
  const deleteMutation = useDeleteApiKey();

  async function handleDelete() {
    try {
      await deleteMutation.mutateAsync(keyId);
      toast.success("API key revoked");
      void navigate({ to: "/keys", search: { tab: "nyxid" } });
    } catch {
      toast.error("Failed to revoke key");
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Revoke API Key</DialogTitle>
          <DialogDescription>
            Are you sure you want to revoke &quot;{keyName}&quot;? This action
            cannot be undone and any applications using this key will stop
            working.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={() => void handleDelete()}
            disabled={deleteMutation.isPending}
          >
            Revoke Key
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function ApiKeyDetailPage() {
  const { keyId } = useParams({ strict: false }) as { keyId: string };
  const { data: apiKey, isLoading, error } = useApiKey(keyId);
  const [rotateOpen, setRotateOpen] = useState(false);
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

  if (error || !apiKey) {
    return (
      <div className="space-y-8">
        <PageHeader
          title="Key Not Found"
          breadcrumbs={[
            { label: "AI Services", to: "/keys" },
            { label: "API Keys", to: "/keys?tab=nyxid" },
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

  return (
    <div className="space-y-8">
      <PageHeader
        title={apiKey.name}
        description={
          apiKey.description ?? `API key ${maskApiKey(apiKey.key_prefix)}`
        }
        breadcrumbs={[
          { label: "AI Services", to: "/keys" },
          { label: "API Keys", to: "/keys?tab=nyxid" },
          { label: apiKey.name },
        ]}
        actions={
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setRotateOpen(true)}
              disabled={!apiKey.is_active}
            >
              <RefreshCw className="mr-2 h-4 w-4" />
              Rotate Key
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => setDeleteOpen(true)}
            >
              <Trash2 className="mr-2 h-4 w-4" />
              Revoke
            </Button>
          </div>
        }
      />

      <div className="grid gap-4 md:grid-cols-2">
        <DetailsCard
          name={apiKey.name}
          description={apiKey.description}
          keyPrefix={apiKey.key_prefix}
          scopes={apiKey.scopes}
          isActive={apiKey.is_active}
          createdAt={apiKey.created_at}
          lastUsedAt={apiKey.last_used_at}
          expiresAt={apiKey.expires_at}
          keyId={apiKey.id}
        />

        <ServiceScopeCard
          keyId={apiKey.id}
          allowAllServices={apiKey.allow_all_services}
          allowedServiceIds={apiKey.allowed_service_ids}
          allowedServices={apiKey.allowed_services}
        />

        <NodeScopeCard
          keyId={apiKey.id}
          allowAllNodes={apiKey.allow_all_nodes}
          allowedNodeIds={apiKey.allowed_node_ids}
          allowedNodes={apiKey.allowed_nodes}
        />
      </div>

      <RotateKeyDialog
        open={rotateOpen}
        onOpenChange={setRotateOpen}
        keyId={apiKey.id}
      />

      <DeleteKeyDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        keyId={apiKey.id}
        keyName={apiKey.name}
      />
    </div>
  );
}
