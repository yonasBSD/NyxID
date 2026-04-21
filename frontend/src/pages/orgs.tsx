import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { Building2, Plus, Users } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { PageHeader } from "@/components/shared/page-header";
import { formatRelativeTime } from "@/lib/utils";
import { useOrgs } from "@/hooks/use-orgs";
import { CreateOrgDialog } from "@/components/orgs/create-org-dialog";
import { OrgAvatar } from "@/components/orgs/org-avatar";
import { RoleBadge } from "@/components/orgs/role-badge";

function OrgsLoading() {
  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {Array.from({ length: 3 }).map((_, i) => (
        <Skeleton key={`org-skel-${String(i)}`} className="h-28 rounded-xl" />
      ))}
    </div>
  );
}

function OrgsEmptyState({ onCreate }: { readonly onCreate: () => void }) {
  return (
    <Card>
      <CardContent className="flex flex-col items-center gap-4 py-16">
        <div className="flex h-14 w-14 items-center justify-center rounded-full border border-border">
          <Building2 className="h-6 w-6 text-muted-foreground" />
        </div>
        <div className="text-center">
          <p className="text-sm font-medium">No organizations yet</p>
          <p className="text-xs text-muted-foreground">
            Create an organization to share services and credentials with
            teammates.
          </p>
        </div>
        <Button size="sm" onClick={onCreate}>
          <Plus className="mr-2 h-4 w-4" />
          Create your first organization
        </Button>
      </CardContent>
    </Card>
  );
}

export function OrgsPage() {
  const { data: orgs, isLoading, error } = useOrgs();
  const [createOpen, setCreateOpen] = useState(false);

  return (
    <div className="space-y-8">
      <PageHeader
        title="Organizations"
        description="Organizations let you share services and credentials with a team."
        actions={
          <Button size="sm" onClick={() => setCreateOpen(true)}>
            <Plus className="mr-2 h-4 w-4" />
            New organization
          </Button>
        }
      />

      {isLoading ? (
        <OrgsLoading />
      ) : error ? (
        <Card>
          <CardContent className="py-8 text-center text-sm text-destructive">
            Failed to load organizations. Please try again.
          </CardContent>
        </Card>
      ) : !orgs || orgs.length === 0 ? (
        <OrgsEmptyState onCreate={() => setCreateOpen(true)} />
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {orgs.map((org) => (
            <Link
              key={org.id}
              to="/orgs/$orgId"
              params={{ orgId: org.id }}
              className="block focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-xl"
            >
              <Card className="transition-colors hover:border-primary/30 hover:bg-accent/30">
                <CardContent className="flex flex-col gap-3 p-5">
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex min-w-0 items-center gap-3">
                      <OrgAvatar
                        avatarUrl={org.avatar_url}
                        displayName={org.display_name}
                        className="h-10 w-10"
                      />
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium text-foreground">
                          {org.display_name ?? "Untitled org"}
                        </p>
                        {org.contact_email && (
                          <p className="truncate text-xs text-muted-foreground">
                            {org.contact_email}
                          </p>
                        )}
                        <p className="text-xs text-muted-foreground">
                          Created {formatRelativeTime(org.created_at) ?? "—"}
                        </p>
                      </div>
                    </div>
                    <RoleBadge role={org.your_role} />
                  </div>
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Users className="h-3.5 w-3.5" />
                    <span>View members and invites</span>
                  </div>
                </CardContent>
              </Card>
            </Link>
          ))}
        </div>
      )}

      <CreateOrgDialog open={createOpen} onOpenChange={setCreateOpen} />
    </div>
  );
}
