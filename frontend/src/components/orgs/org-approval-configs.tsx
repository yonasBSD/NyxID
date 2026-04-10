import { useMemo, useState } from "react";
import { Shield, RotateCcw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Skeleton } from "@/components/ui/skeleton";
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
import { ApiError } from "@/lib/api-client";
import {
  useServiceApprovalConfigs,
  useSetServiceApprovalConfig,
  useDeleteServiceApprovalConfig,
  useApprovalGrants,
  useRevokeGrant,
} from "@/hooks/use-approvals";
import { useKeys } from "@/hooks/use-keys";
import { formatDate } from "@/lib/utils";
import type { ApprovalMode } from "@/types/approvals";

interface OrgApprovalConfigsProps {
  readonly orgId: string;
}

/**
 * Admin-only UI for setting per-service approval policies on org-owned
 * services. When a policy is set on an org service, the org's policy is
 * dominant over any actor's personal gate: every member of the org will
 * require approval by the primary admin(s) of the org before their proxy
 * call goes through.
 */
export function OrgApprovalConfigs({ orgId }: OrgApprovalConfigsProps) {
  const { data: keys, isLoading: isKeysLoading } = useKeys();
  const {
    data: serviceConfigs,
    isLoading: isConfigsLoading,
    error: configsError,
  } = useServiceApprovalConfigs(orgId);
  const setConfigMutation = useSetServiceApprovalConfig();
  const deleteConfigMutation = useDeleteServiceApprovalConfig();

  const [addDialogOpen, setAddDialogOpen] = useState(false);
  const [selectedServiceId, setSelectedServiceId] = useState("");
  const [selectedApprovalRequired, setSelectedApprovalRequired] =
    useState(true);
  const [selectedApprovalMode, setSelectedApprovalMode] =
    useState<ApprovalMode>("per_request");

  // Only org-owned services for this org -- personal services show up under
  // /notifications. We also hide disabled keys so admins do not configure
  // a policy on a service that no-one can use, and require a
  // `catalog_service_id` because the approval config endpoint is keyed by
  // the catalog `DownstreamService.id`, not the per-user `UserService.id`.
  // Custom-only services (no catalog binding) cannot have a per-service
  // approval policy and are excluded from the picker.
  const orgServices = useMemo(() => {
    return (keys ?? []).filter((key) => {
      const src = key.credential_source;
      return (
        key.is_active &&
        key.catalog_service_id !== null &&
        src !== undefined &&
        src.type === "org" &&
        src.org_id === orgId
      );
    });
  }, [keys, orgId]);

  // `serviceConfigs.configs[].service_id` and the picker option values
  // are both in `DownstreamService.id` space, so dedupe through that
  // identifier.
  const configuredCatalogIds = useMemo(
    () => new Set((serviceConfigs?.configs ?? []).map((c) => c.service_id)),
    [serviceConfigs],
  );

  const availableServices = useMemo(
    () =>
      orgServices.filter(
        (s) =>
          s.catalog_service_id !== null &&
          !configuredCatalogIds.has(s.catalog_service_id),
      ),
    [orgServices, configuredCatalogIds],
  );

  async function handleAdd() {
    if (!selectedServiceId) return;
    try {
      await setConfigMutation.mutateAsync({
        serviceId: selectedServiceId,
        approvalRequired: selectedApprovalRequired,
        approvalMode: selectedApprovalMode,
        orgId,
      });
      toast.success("Org approval policy added");
      setAddDialogOpen(false);
      setSelectedServiceId("");
      setSelectedApprovalRequired(true);
      setSelectedApprovalMode("per_request");
    } catch (err) {
      toast.error(
        err instanceof ApiError ? err.message : "Failed to add policy",
      );
    }
  }

  async function handleToggle(serviceId: string, approvalRequired: boolean) {
    try {
      const existing = serviceConfigs?.configs.find(
        (c) => c.service_id === serviceId,
      );
      await setConfigMutation.mutateAsync({
        serviceId,
        approvalRequired,
        approvalMode: existing?.approval_mode,
        orgId,
      });
    } catch (err) {
      toast.error(
        err instanceof ApiError ? err.message : "Failed to update policy",
      );
    }
  }

  async function handleChangeMode(
    serviceId: string,
    approvalRequired: boolean,
    approvalMode: ApprovalMode,
  ) {
    try {
      await setConfigMutation.mutateAsync({
        serviceId,
        approvalRequired,
        approvalMode,
        orgId,
      });
    } catch (err) {
      toast.error(
        err instanceof ApiError ? err.message : "Failed to update approval mode",
      );
    }
  }

  async function handleDelete(serviceId: string) {
    try {
      await deleteConfigMutation.mutateAsync({ serviceId, orgId });
      toast.success("Org approval policy removed");
    } catch (err) {
      toast.error(
        err instanceof ApiError ? err.message : "Failed to remove policy",
      );
    }
  }

  const loading = isKeysLoading || isConfigsLoading;

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="flex items-center gap-2">
                <Shield className="h-5 w-5" aria-hidden="true" />
                Org Approval Policies
              </CardTitle>
              <CardDescription>
                Require approval when members of this org use shared services.
                When an org policy is set, it takes precedence over the
                member&rsquo;s personal approval settings, and admins of this
                org are notified to decide.
              </CardDescription>
            </div>
            <Button
              size="sm"
              variant="outline"
              onClick={() => setAddDialogOpen(true)}
              disabled={
                loading ||
                Boolean(configsError) ||
                availableServices.length === 0
              }
            >
              Add policy
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {loading ? (
            <div className="space-y-3 py-2">
              <Skeleton className="h-16 w-full" />
              <Skeleton className="h-16 w-full" />
            </div>
          ) : configsError ? (
            <p className="py-4 text-center text-sm text-muted-foreground">
              Failed to load org approval policies. Try refreshing the page.
            </p>
          ) : orgServices.length === 0 ? (
            <p className="py-4 text-center text-sm text-muted-foreground">
              No org-owned services yet. Add a key to this org before
              configuring approval policies.
            </p>
          ) : serviceConfigs?.configs.length === 0 ? (
            <p className="py-4 text-center text-sm text-muted-foreground">
              No org approval policies configured. Members use their personal
              approval settings.
            </p>
          ) : (
            <div className="space-y-3">
              {serviceConfigs?.configs.map((config) => (
                <div
                  key={config.service_id}
                  className="rounded-lg border border-border p-4"
                >
                  <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                      <p className="text-sm font-medium">
                        {config.service_name}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        {config.approval_required
                          ? "Approval required (org policy dominant)"
                          : "Approval not required (org policy)"}
                      </p>
                    </div>
                    <div className="flex items-center gap-3">
                      <Switch
                        checked={config.approval_required}
                        onCheckedChange={(checked) =>
                          void handleToggle(config.service_id, checked)
                        }
                      />
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => void handleDelete(config.service_id)}
                        title="Remove org policy"
                      >
                        <RotateCcw className="h-4 w-4 text-muted-foreground" />
                      </Button>
                    </div>
                  </div>
                  {config.approval_required && (
                    <div className="mt-3 flex items-center gap-2 border-t border-border pt-3">
                      <span className="text-xs text-muted-foreground">
                        Mode:
                      </span>
                      <Select
                        value={config.approval_mode}
                        onValueChange={(value) =>
                          void handleChangeMode(
                            config.service_id,
                            config.approval_required,
                            value as ApprovalMode,
                          )
                        }
                      >
                        <SelectTrigger className="h-7 w-[180px] text-xs">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="per_request">
                            Per request
                          </SelectItem>
                          <SelectItem value="grant">
                            Time-based grant
                          </SelectItem>
                        </SelectContent>
                      </Select>
                      <span className="text-xs text-muted-foreground">
                        {config.approval_mode === "grant"
                          ? "Approval creates a reusable grant"
                          : "Every request needs fresh approval"}
                      </span>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <OrgApprovalGrants orgId={orgId} />

      <Dialog open={addDialogOpen} onOpenChange={setAddDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Add Org Approval Policy</DialogTitle>
            <DialogDescription>
              Choose an org-owned service and configure whether approval is
              required for any member using it.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <label
                className="text-sm font-medium"
                htmlFor="org-service-select"
              >
                Service
              </label>
              <Select
                value={selectedServiceId}
                onValueChange={setSelectedServiceId}
              >
                <SelectTrigger id="org-service-select">
                  <SelectValue placeholder="Select a service" />
                </SelectTrigger>
                <SelectContent>
                  {availableServices.map((s) => (
                    // The Select stores `catalog_service_id` because the
                    // backend `/approvals/service-configs/{id}` endpoint
                    // expects a `DownstreamService.id`. The
                    // `availableServices` filter above guarantees this is
                    // non-null.
                    <SelectItem
                      key={s.catalog_service_id ?? s.id}
                      value={s.catalog_service_id ?? ""}
                    >
                      {s.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex items-center justify-between rounded-lg border border-border p-4">
              <div className="space-y-0.5">
                <p className="text-sm font-medium">Require Approval</p>
                <p className="text-xs text-muted-foreground">
                  Whether members must get an admin&rsquo;s approval to use
                  this service.
                </p>
              </div>
              <Switch
                checked={selectedApprovalRequired}
                onCheckedChange={setSelectedApprovalRequired}
              />
            </div>
            {selectedApprovalRequired && (
              <div className="space-y-2">
                <label
                  className="text-sm font-medium"
                  htmlFor="org-approval-mode-select"
                >
                  Approval Mode
                </label>
                <Select
                  value={selectedApprovalMode}
                  onValueChange={(v) =>
                    setSelectedApprovalMode(v as ApprovalMode)
                  }
                >
                  <SelectTrigger id="org-approval-mode-select">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="per_request">Per request</SelectItem>
                    <SelectItem value="grant">Time-based grant</SelectItem>
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground">
                  {selectedApprovalMode === "grant"
                    ? "Approval creates a reusable grant lasting the configured number of days. Subsequent requests skip approval until the grant expires."
                    : "Every API request requires a fresh approval. No grant is created."}
                </p>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAddDialogOpen(false)}>
              Cancel
            </Button>
            <Button
              onClick={() => void handleAdd()}
              disabled={!selectedServiceId}
              isLoading={setConfigMutation.isPending}
            >
              Add policy
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

interface OrgApprovalGrantsProps {
  readonly orgId: string;
}

/**
 * Org-scoped grants list. Grants are only created when an org-policy
 * approval is decided in `grant` mode -- they live under the org's
 * user_id, so without this admin-only panel they would be unreachable.
 */
function OrgApprovalGrants({ orgId }: OrgApprovalGrantsProps) {
  const { data, isLoading, error } = useApprovalGrants(1, 50, orgId);
  const revokeMutation = useRevokeGrant();
  const grants = data?.grants ?? [];

  async function handleRevoke(grantId: string) {
    try {
      await revokeMutation.mutateAsync({ grantId, orgId });
      toast.success("Grant revoked");
    } catch (err) {
      toast.error(
        err instanceof ApiError ? err.message : "Failed to revoke grant",
      );
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Shield className="h-5 w-5" aria-hidden="true" />
          Org Approval Grants
        </CardTitle>
        <CardDescription>
          Active reusable grants created when org approval policies in
          time-based grant mode have been decided. Revoking a grant forces
          the next member call to require fresh approval.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="space-y-3 py-2">
            <Skeleton className="h-14 w-full" />
            <Skeleton className="h-14 w-full" />
          </div>
        ) : error ? (
          <p className="py-4 text-center text-sm text-muted-foreground">
            Failed to load org approval grants. Try refreshing the page.
          </p>
        ) : grants.length === 0 ? (
          <p className="py-4 text-center text-sm text-muted-foreground">
            No active org approval grants.
          </p>
        ) : (
          <div className="space-y-3">
            {grants.map((grant) => (
              <div
                key={grant.id}
                className="flex items-center justify-between rounded-lg border border-border p-4"
              >
                <div className="space-y-0.5">
                  <p className="text-sm font-medium">{grant.service_name}</p>
                  <p className="text-xs text-muted-foreground">
                    {grant.requester_label ?? grant.requester_type} ·
                    granted {formatDate(grant.granted_at)} · expires{" "}
                    {formatDate(grant.expires_at)}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => void handleRevoke(grant.id)}
                  title="Revoke grant"
                >
                  <Trash2 className="h-4 w-4 text-muted-foreground" />
                </Button>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
