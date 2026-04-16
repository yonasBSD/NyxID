import { useMemo, useState } from "react";
import { toast } from "sonner";
import { Globe, KeyRound } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError } from "@/lib/api-client";
import { useKeys } from "@/hooks/use-keys";
import { useUpdateMember } from "@/hooks/use-org-members";
import type { MemberResponse } from "@/schemas/orgs";
import type { KeyInfo } from "@/types/keys";

interface MemberScopeDialogProps {
  readonly orgId: string;
  readonly member: MemberResponse | null;
  readonly onOpenChange: (open: boolean) => void;
}

/**
 * Edit `allowed_service_ids` for a single member of an org. Admins use this
 * to restrict which org services a member can proxy through, or clear the
 * restriction to grant full access to every org service.
 *
 * The picker lists every service visible to the admin under the given org
 * (i.e. services whose `credential_source` is this org). Each service is a
 * checkbox; unchecking all services implicitly becomes "no access" and the
 * backend enforces that at proxy time. To restore unrestricted access, use
 * the "Allow all services" toggle which clears the `allowed_service_ids`
 * field back to `null`.
 */
export function MemberScopeDialog({
  orgId,
  member,
  onOpenChange,
}: MemberScopeDialogProps) {
  return (
    <Dialog open={member !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        {member && (
          // Key by membership id so reopening the dialog for a different
          // member fully resets the local form state. This avoids the
          // "sync props into state via useEffect" footgun that React 19's
          // eslint rule flags as a cascading render.
          <MemberScopeForm
            key={member.membership_id}
            orgId={orgId}
            member={member}
            onClose={() => onOpenChange(false)}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function MemberScopeForm({
  orgId,
  member,
  onClose,
}: {
  readonly orgId: string;
  readonly member: MemberResponse;
  readonly onClose: () => void;
}) {
  const { data: keys, isLoading: keysLoading } = useKeys();
  const updateMutation = useUpdateMember();

  const orgServices = useMemo(
    (): readonly KeyInfo[] =>
      (keys ?? []).filter(
        (k) =>
          k.credential_source?.type === "org" &&
          k.credential_source.org_id === orgId,
      ),
    [keys, orgId],
  );

  // Seed local state from the member's current scope. `null` on the wire
  // means "no restriction" which the dialog presents as the allow-all toggle.
  const initialScope = member.allowed_service_ids;
  const [allowAll, setAllowAll] = useState(initialScope === null);
  const [selectedIds, setSelectedIds] = useState<readonly string[]>(
    initialScope ?? [],
  );

  function toggleService(serviceId: string) {
    setSelectedIds((prev) =>
      prev.includes(serviceId)
        ? prev.filter((id) => id !== serviceId)
        : [...prev, serviceId],
    );
  }

  async function handleSave() {
    try {
      await updateMutation.mutateAsync({
        orgId,
        memberId: member.user_id,
        // `null` clears the scope (full access). A concrete array restricts.
        body: {
          allowed_service_ids: allowAll ? null : [...selectedIds],
        },
      });
      toast.success("Service access updated");
      onClose();
    } catch (err) {
      toast.error(
        err instanceof ApiError ? err.message : "Failed to update scope",
      );
    }
  }

  const memberName = member.display_name ?? member.email ?? member.user_id;

  return (
    <>
      <DialogHeader>
        <DialogTitle>Edit service access</DialogTitle>
        <DialogDescription>
          Choose which org services <strong>{memberName}</strong> is allowed
          to proxy through. Admins always have full access.
        </DialogDescription>
      </DialogHeader>

      <div className="space-y-4">
        <div className="flex items-start gap-3 rounded-lg border border-border bg-muted/30 p-3">
          <Checkbox
            id="member-scope-allow-all"
            checked={allowAll}
            onCheckedChange={(checked) => setAllowAll(checked === true)}
            className="mt-0.5"
          />
          <div className="space-y-1">
            <Label
              htmlFor="member-scope-allow-all"
              className="cursor-pointer text-sm font-medium"
            >
              Allow all org services
            </Label>
            <p className="text-xs text-muted-foreground">
              When enabled, the member can use every current and future
              service this org owns.
            </p>
          </div>
        </div>

        {!allowAll && (
          <div className="space-y-2">
            <Label className="text-xs font-medium text-muted-foreground">
              Services
            </Label>
            {keysLoading ? (
              <div className="space-y-2">
                <Skeleton className="h-8 w-full" />
                <Skeleton className="h-8 w-full" />
                <Skeleton className="h-8 w-full" />
              </div>
            ) : orgServices.length === 0 ? (
              <div className="rounded-lg border border-dashed border-border p-4 text-center text-xs text-muted-foreground">
                This org has no services yet. Add one under AI Services first.
              </div>
            ) : (
              <div className="max-h-64 space-y-1 overflow-y-auto rounded-lg border border-border p-2">
                {orgServices.map((service) => {
                  const id = `member-scope-svc-${service.id}`;
                  const isChecked = selectedIds.includes(service.id);
                  return (
                    <div
                      key={service.id}
                      className="flex items-start gap-3 rounded px-2 py-1.5 hover:bg-accent/40"
                    >
                      <Checkbox
                        id={id}
                        checked={isChecked}
                        onCheckedChange={() => toggleService(service.id)}
                        className="mt-1"
                      />
                      <Label
                        htmlFor={id}
                        className="flex-1 cursor-pointer space-y-0.5"
                      >
                        <span className="block text-sm font-medium text-foreground">
                          {service.label}
                        </span>
                        <span className="flex items-center gap-2 text-xs text-muted-foreground">
                          {service.service_type === "ssh" ? (
                            <KeyRound className="h-3 w-3" aria-hidden />
                          ) : (
                            <Globe className="h-3 w-3" aria-hidden />
                          )}
                          <span className="font-mono">{service.slug}</span>
                        </span>
                      </Label>
                    </div>
                  );
                })}
              </div>
            )}
            {orgServices.length > 0 && (
              <p className="text-[11px] text-muted-foreground">
                Selecting nothing revokes proxy access to every org service.
              </p>
            )}
          </div>
        )}
      </div>

      <DialogFooter>
        <Button
          variant="outline"
          onClick={onClose}
          disabled={updateMutation.isPending}
        >
          Cancel
        </Button>
        <Button
          onClick={() => void handleSave()}
          isLoading={updateMutation.isPending}
        >
          Save
        </Button>
      </DialogFooter>
    </>
  );
}
