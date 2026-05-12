import { useParams } from "@tanstack/react-router";
import { ServiceAccountDetail } from "@/components/service-accounts/service-account-detail";
import { useAuthStore } from "@/stores/auth-store";
import { canAdminWrite } from "@/types/api";

export function AdminServiceAccountDetailPage() {
  const { saId } = useParams({
    from: "/dashboard/admin/service-accounts/$saId",
  });
  const currentUser = useAuthStore((s) => s.user);
  const canWrite = canAdminWrite(currentUser);

  return (
    <ServiceAccountDetail
      saId={saId}
      backTo={{ to: "/admin/service-accounts", label: "Service Accounts" }}
      showProviderSections
      canWrite={canWrite}
    />
  );
}
