import { useState } from "react";
import {
  useAgentBindings,
  useCreateBinding,
  useDeleteBinding,
} from "@/hooks/use-agent-bindings";
import { useExternalApiKeys, useKeys } from "@/hooks/use-keys";
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
  const { data: externalApiKeys } = useExternalApiKeys();
  const createBinding = useCreateBinding();
  const deleteBinding = useDeleteBinding();
  const [adding, setAdding] = useState(false);
  const [selectedServiceId, setSelectedServiceId] = useState("");
  const [selectedCredentialId, setSelectedCredentialId] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<AgentServiceBinding | null>(
    null,
  );

  function handleCreate() {
    if (!selectedServiceId || !selectedCredentialId) return;
    createBinding.mutate(
      {
        keyId,
        user_service_id: selectedServiceId,
        user_api_key_id: selectedCredentialId,
      },
      {
        onSuccess: () => {
          toast.success("Credential binding created");
          setAdding(false);
          setSelectedServiceId("");
          setSelectedCredentialId("");
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

  const services = allKeys ?? [];
  const credentials = externalApiKeys ?? [];

  return (
    <Card className="md:col-span-2">
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Link2 className="h-4 w-4 text-primary" />
            <CardTitle className="text-sm">Credential Bindings</CardTitle>
          </div>
          <Button
            size="sm"
            variant="outline"
            onClick={() => setAdding(true)}
            disabled={adding}
          >
            Add Binding
          </Button>
        </div>
        <CardDescription>
          Override which credential is used when this agent accesses a service.
          Without a binding, the service default credential is used.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {adding && (
          <div className="rounded-lg border border-border p-3 space-y-3">
            <div className="space-y-1.5">
              <Label className="text-xs">Service</Label>
              <Select
                value={selectedServiceId}
                onValueChange={setSelectedServiceId}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select service" />
                </SelectTrigger>
                <SelectContent>
                  {services.map((s) => (
                    <SelectItem key={s.id} value={s.id}>
                      {s.label} ({s.slug})
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <Label className="text-xs">Override credential</Label>
              <Select
                value={selectedCredentialId}
                onValueChange={setSelectedCredentialId}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select credential" />
                </SelectTrigger>
                <SelectContent>
                  {credentials.map((credential) => (
                    <SelectItem key={credential.id} value={credential.id}>
                      {credential.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex gap-2">
              <Button
                size="sm"
                onClick={handleCreate}
                disabled={
                  createBinding.isPending ||
                  !selectedServiceId ||
                  !selectedCredentialId
                }
              >
                Create
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  setAdding(false);
                  setSelectedServiceId("");
                  setSelectedCredentialId("");
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
                    {b.service_label}{" "}
                    <span className="text-muted-foreground">
                      ({b.service_slug})
                    </span>
                  </p>
                  <p className="text-xs text-muted-foreground">
                    Uses: {b.credential_label}
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
            No credential bindings. This agent uses default credentials for all
            services.
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
              Remove the credential override for &quot;
              {deleteTarget?.service_label}&quot;? This agent will revert to
              using the service default credential.
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
