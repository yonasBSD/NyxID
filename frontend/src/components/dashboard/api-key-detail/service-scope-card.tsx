import { useState } from "react";
import { useUpdateApiKey } from "@/hooks/use-api-keys";
import { useKeys } from "@/hooks/use-keys";
import { ApiError } from "@/lib/api-client";
import { Button } from "@/components/ui/button";
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
import { Shield, Pencil } from "lucide-react";
import { toast } from "sonner";

export function ServiceScopeCard({
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
