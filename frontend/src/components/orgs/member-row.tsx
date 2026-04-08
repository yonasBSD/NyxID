import { Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { TableCell, TableRow } from "@/components/ui/table";
import { formatRelativeTime } from "@/lib/utils";
import type { MemberResponse, OrgRole } from "@/schemas/orgs";
import { ORG_ROLES } from "@/schemas/orgs";
import { RoleBadge } from "./role-badge";

interface MemberRowProps {
  readonly member: MemberResponse;
  readonly canManage: boolean;
  readonly isSelf: boolean;
  readonly isUpdating: boolean;
  readonly onChangeRole: (memberId: string, nextRole: OrgRole) => void;
  readonly onRevoke: (member: MemberResponse) => void;
}

/**
 * A single row in the org members table. When the caller is not an admin
 * (`canManage === false`), the role is shown as a read-only badge and the
 * revoke button is hidden.
 */
export function MemberRow({
  member,
  canManage,
  isSelf,
  isUpdating,
  onChangeRole,
  onRevoke,
}: MemberRowProps) {
  const displayName =
    member.display_name ?? member.email ?? member.user_id;

  return (
    <TableRow>
      <TableCell>
        <div className="flex flex-col gap-0.5">
          <span className="text-sm font-medium text-foreground">
            {displayName}
            {isSelf && (
              <span className="ml-2 text-xs text-muted-foreground">(you)</span>
            )}
          </span>
          {member.email && member.display_name && (
            <span className="text-xs text-muted-foreground">
              {member.email}
            </span>
          )}
        </div>
      </TableCell>
      <TableCell>
        {canManage ? (
          <Select
            value={member.role}
            onValueChange={(next) => onChangeRole(member.user_id, next as OrgRole)}
            disabled={isUpdating}
          >
            <SelectTrigger className="h-8 w-[120px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ORG_ROLES.map((role) => (
                <SelectItem key={role} value={role}>
                  {role.charAt(0).toUpperCase() + role.slice(1)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : (
          <RoleBadge role={member.role} />
        )}
      </TableCell>
      <TableCell className="text-muted-foreground">
        {formatRelativeTime(member.created_at) ?? "—"}
      </TableCell>
      <TableCell>
        {canManage && (
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-muted-foreground hover:text-destructive"
            onClick={() => onRevoke(member)}
            disabled={isUpdating}
            aria-label={`Remove ${displayName}`}
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        )}
      </TableCell>
    </TableRow>
  );
}
