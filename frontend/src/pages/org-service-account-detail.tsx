import { useParams } from "@tanstack/react-router";
import { ServiceAccountDetail } from "@/components/service-accounts/service-account-detail";
import { useOrg } from "@/hooks/use-orgs";

export function OrgServiceAccountDetailPage() {
  const { orgId, saId } = useParams({
    from: "/dashboard/orgs/$orgId/service-accounts/$saId",
  });
  const { data: org } = useOrg(orgId);

  return (
    <ServiceAccountDetail
      saId={saId}
      backTo={{
        to: `/orgs/${orgId}`,
        label: org?.display_name ?? "Organization",
      }}
      showProviderSections={false}
    />
  );
}
