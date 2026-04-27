import { useParams } from "@tanstack/react-router";
import { DeveloperAppDetail } from "@/components/developer-apps/developer-app-detail";
import { useOrg } from "@/hooks/use-orgs";

export function OrgDeveloperAppDetailPage() {
  const { orgId, clientId } = useParams({
    from: "/dashboard/orgs/$orgId/developer-apps/$clientId",
  });
  const { data: org } = useOrg(orgId);

  return (
    <DeveloperAppDetail
      clientId={clientId}
      backTo={{
        to: `/orgs/${orgId}`,
        label: org?.display_name ?? "Organization",
      }}
    />
  );
}
