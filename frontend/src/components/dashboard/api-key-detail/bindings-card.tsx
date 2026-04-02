import { useState } from "react";
import {
  useAgentBindings,
  useCreateBinding,
  useDeleteBinding,
} from "@/hooks/use-agent-bindings";
import { useKeys } from "@/hooks/use-keys";
import { ApiError } from "@/lib/api-client";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
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
import { Link2, Trash2 } from "lucide-react";
import { toast } from "sonner";
import type { AgentServiceBinding } from "@/types/keys";

export function BindingsCard({
  keyId,
}: {
  readonly keyId: string;
}) {
  const { data: bindings, isLoading } = useAgentBindings(keyId);
  const { data: allKeys } = useKeys();
  const createBinding = useCreateBinding();
  const deleteBinding = useDeleteBinding();
  const [adding, setAdding] = useState(false);
  const [selectedServiceId, setSelectedServiceId] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<AgentServiceBinding | null>(
    null,
  );

  // Only show services that have a credential configured
  const services = (allKeys ?? []).filter((s) => s.api_key_id);

  // Already bound service IDs (to exclude from the dropdown)
  const boundServiceIds = new Set(
    (bindings ?? []).map((b) => b.user_service_id),
  );
  const availableServices = services.filter(
    (s) => !boundServiceIds.has(s.id),
  );

  function handleCreate() {
    if (!selectedServiceId) return;
    const service = services.find((s) => s.id === selectedServiceId);
    if (!service?.api_key_id) {
      toast.error("Selected service has no credential configured");
      return;
    }
    createBinding.mutate(
      {
        keyId,
        user_service_id: selectedServiceId,
        user_api_key_id: service.api_key_id,
      },
      {
        onSuccess: () => {
          toast.success("Service binding created");
          setAdding(false);
          setSelectedServiceId("");
        },
        onError: (err) => {
          const message =
            err instanceof ApiError
              ? err.message
              : "Failed to create binding";
          toast.error(message);
        },
      },
    );
  }

  function handleDelete() {
    if (!deleteTarget) return;
    deleteBinding.mutate(
      { keyId, bindingId: deleteTarget.id },
      {
        onSuccess: () => {
          toast.success("Binding removed");
          setDeleteTarget(null);
        },
        onError: (err) => {
          const message =
            err instanceof ApiError
              ? err.message
              : "Failed to remove binding";
          toast.error(message);
        },
      },
    );
  }

  return (
    <Card className="md:col-span-2">
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Link2 className="h-4 w-4 text-primary" />
            <CardTitle className="text-sm">Service Bindings</CardTitle>
          </div>
          <Button
            size="sm"
            variant="outline"
            onClick={() => setAdding(true)}
            disabled={adding || availableServices.length === 0}
          >
            Add Service
          </Button>
        </div>
        <CardDescription>
          Choose which AI services this agent can access. The service's
          credential is used automatically.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {adding && (
          <div className="rounded-lg border border-border p-3 space-y-3">
            <div className="space-y-1.5">
              <Label className="text-xs">AI Service</Label>
              <Select
                value={selectedServiceId}
                onValueChange={setSelectedServiceId}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select a service" />
                </SelectTrigger>
                <SelectContent>
                  {availableServices.map((s) => (
                    <SelectItem key={s.id} value={s.id}>
                      {s.label}
                      {s.slug !== s.label ? ` (${s.slug})` : ""}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex gap-2">
              <Button
                size="sm"
                onClick={handleCreate}
                disabled={createBinding.isPending || !selectedServiceId}
              >
                Add
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  setAdding(false);
                  setSelectedServiceId("");
                }}
              >
                Cancel
              </Button>
            </div>
          </div>
        )}

        {isLoading ? (
          <div className="space-y-2">
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
          </div>
        ) : bindings && bindings.length > 0 ? (
          <div className="space-y-2">
            {bindings.map((b) => (
              <div
                key={b.id}
                className="flex items-center justify-between rounded-lg border border-border p-3"
              >
                <div className="space-y-0.5">
                  <p className="text-sm font-medium">
                    {b.service_label}
                    {b.service_slug !== b.service_label && (
                      <span className="text-muted-foreground">
                        {" "}({b.service_slug})
                      </span>
                    )}
                  </p>
                </div>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-7 w-7 text-destructive hover:text-destructive"
                  onClick={() => setDeleteTarget(b)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">
            No service bindings. Add AI services to control which services this
            agent can access.
          </p>
        )}
      </CardContent>

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Remove Binding</DialogTitle>
            <DialogDescription>
              Remove &quot;{deleteTarget?.service_label}&quot; from this
              agent's service bindings?
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={deleteBinding.isPending}
            >
              Remove
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
