import { Badge } from "@/components/ui/badge";
import type { OrgRole } from "@/schemas/orgs";

interface RoleBadgeProps {
  readonly role: OrgRole;
  readonly className?: string;
}

const ROLE_LABEL: Record<OrgRole, string> = {
  admin: "Admin",
  member: "Member",
  viewer: "Viewer",
};

/**
 * Small pill showing an org role. Colors follow the shared Badge variants so
 * they adapt to light/dark themes.
 */
export function RoleBadge({ role, className }: RoleBadgeProps) {
  const variant: "info" | "success" | "secondary" =
    role === "admin"
      ? "info"
      : role === "member"
        ? "success"
        : "secondary";

  return (
    <Badge variant={variant} className={className}>
      {ROLE_LABEL[role]}
    </Badge>
  );
}
