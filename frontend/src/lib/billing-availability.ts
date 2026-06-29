import type { User } from "@/types/api";
import { isBillingAvailable } from "@/types/api";

type BillingAvailabilityUser = Pick<User, "capabilities"> | null;

export function shouldRedirectFromBilling(auth: {
  readonly isLoading: boolean;
  readonly user: BillingAvailabilityUser;
}): boolean {
  return !auth.isLoading && !isBillingAvailable(auth.user);
}
