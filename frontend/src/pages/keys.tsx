import { useState } from "react";
import { Link, useSearch, useNavigate } from "@tanstack/react-router";
import { useKeys } from "@/hooks/use-keys";
import { PageHeader } from "@/components/shared/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Plus, Globe, KeyRound, Server, Router, Terminal, Zap } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { AddKeyDialog } from "@/components/dashboard/add-key-dialog";
import { ApiKeyTable } from "@/components/dashboard/api-key-table";
import { ApiKeyCreateDialog } from "@/components/dashboard/api-key-create-dialog";
import type { KeyInfo } from "@/types/keys";

type TabValue = "services" | "nyxid";

function statusVariant(
  status: string,
): "default" | "secondary" | "destructive" | "outline" {
  switch (status) {
    case "active":
      return "default";
    case "expired":
      return "secondary";
    case "revoked":
    case "refresh_failed":
      return "destructive";
    default:
      return "outline";
  }
}

function KeyCard({ keyInfo }: { readonly keyInfo: KeyInfo }) {
  const isSsh = keyInfo.service_type === "ssh";
  const hasSshCertificateAuth = isSsh && keyInfo.ssh_ca_public_key !== null;
  const displayUrl = isSsh
    ? `${keyInfo.ssh_host ?? "unknown"}:${keyInfo.ssh_port ?? 22}`
    : keyInfo.endpoint_url.length > 50
      ? `${keyInfo.endpoint_url.slice(0, 50)}...`
      : keyInfo.endpoint_url;

  return (
    <Link to="/keys/$keyId" params={{ keyId: keyInfo.id }}>
      <Card className="transition-colors hover:border-primary/30 hover:bg-accent/30">
        <CardContent className="flex flex-col gap-3 p-5">
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <p className="truncate text-sm font-medium text-foreground">
                {keyInfo.label}
              </p>
              {keyInfo.catalog_service_name && (
                <p className="text-xs text-muted-foreground">
                  {keyInfo.catalog_service_name}
                </p>
              )}
            </div>
            <div className="flex shrink-0 items-center gap-2">
              {keyInfo.auto_connected && (
                <Badge variant="secondary">Auto-connected</Badge>
              )}
              {isSsh && <Badge variant="outline">SSH</Badge>}
              <Badge variant={statusVariant(keyInfo.status)}>
                {keyInfo.status}
              </Badge>
              {!keyInfo.is_active && <Badge variant="outline">Inactive</Badge>}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs text-muted-foreground">
            <div className="flex items-center gap-1.5">
              {isSsh ? (
                <Terminal className="h-3 w-3 shrink-0" />
              ) : (
                <Globe className="h-3 w-3 shrink-0" />
              )}
              <span className="truncate">{displayUrl}</span>
            </div>
            <div className="flex items-center gap-1.5">
              {keyInfo.auto_connected ? (
                <Zap className="h-3 w-3 shrink-0" />
              ) : (
                <KeyRound className="h-3 w-3 shrink-0" />
              )}
              <span>
                {keyInfo.auto_connected
                  ? "No auth required"
                  : isSsh
                    ? hasSshCertificateAuth
                      ? "certificate"
                      : "ssh tunnel"
                    : keyInfo.credential_type}
              </span>
            </div>
            <div className="flex items-center gap-1.5">
              <Server className="h-3 w-3 shrink-0" />
              <span className="font-mono">
                {isSsh ? keyInfo.slug : `/proxy/s/${keyInfo.slug}`}
              </span>
            </div>
            <div className="flex items-center gap-1.5">
              <Router className="h-3 w-3 shrink-0" />
              <span>{keyInfo.node_id ? "Via node" : "Direct"}</span>
            </div>
          </div>
        </CardContent>
      </Card>
    </Link>
  );
}

function ServicesEmptyState({ onAdd }: { readonly onAdd: () => void }) {
  return (
    <Card>
      <CardContent className="flex flex-col items-center gap-4 py-16">
        <div className="flex h-14 w-14 items-center justify-center rounded-full border border-border">
          <KeyRound className="h-6 w-6 text-muted-foreground" />
        </div>
        <div className="text-center">
          <p className="text-sm font-medium">No AI services yet</p>
          <p className="text-xs text-muted-foreground">
            Add an AI service to connect to external APIs through NyxID.
          </p>
        </div>
        <Button size="sm" onClick={onAdd}>
          <Plus className="mr-2 h-4 w-4" />
          Add Service
        </Button>
      </CardContent>
    </Card>
  );
}

function LoadingSkeleton() {
  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {Array.from({ length: 6 }, (_, i) => (
        <Skeleton key={i} className="h-32 rounded-xl" />
      ))}
    </div>
  );
}

function ExternalServicesTab({
  onAdd,
  showAutoConnected,
}: {
  readonly onAdd: () => void;
  readonly showAutoConnected: boolean;
}) {
  const { data: keys, isLoading, error } = useKeys();

  if (isLoading) return <LoadingSkeleton />;

  if (error) {
    return (
      <Card>
        <CardContent className="py-8 text-center text-sm text-destructive">
          Failed to load services. Please try again.
        </CardContent>
      </Card>
    );
  }

  const userKeys = (keys ?? []).filter((k) => !k.auto_connected);
  const autoKeys = (keys ?? []).filter((k) => k.auto_connected);
  const visibleKeys = showAutoConnected ? (keys ?? []) : userKeys;

  if (visibleKeys.length === 0 && autoKeys.length === 0) {
    return <ServicesEmptyState onAdd={onAdd} />;
  }

  if (visibleKeys.length === 0) {
    return <ServicesEmptyState onAdd={onAdd} />;
  }

  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {visibleKeys.map((k) => (
        <KeyCard key={k.id} keyInfo={k} />
      ))}
    </div>
  );
}

function NyxIdApiKeysTab() {
  return (
    <div className="space-y-4">
      <div className="flex justify-end">
        <ApiKeyCreateDialog />
      </div>
      <div className="rounded-xl border border-border">
        <ApiKeyTable />
      </div>
    </div>
  );
}

function AddButton({
  tab,
  onAddService,
}: {
  readonly tab: TabValue;
  readonly onAddService: () => void;
}) {
  if (tab === "services") {
    return (
      <Button size="sm" onClick={onAddService}>
        <Plus className="mr-2 h-4 w-4" />
        Add Service
      </Button>
    );
  }
  // "nyxid" tab -- the ApiKeyCreateDialog has its own trigger button
  return null;
}

function AutoConnectedToggle({
  checked,
  onCheckedChange,
  count,
}: {
  readonly checked: boolean;
  readonly onCheckedChange: (checked: boolean) => void;
  readonly count: number;
}) {
  if (count === 0) return null;

  return (
    <div className="flex items-center gap-2">
      <Switch
        id="show-auto-connected"
        checked={checked}
        onCheckedChange={onCheckedChange}
      />
      <Label
        htmlFor="show-auto-connected"
        className="text-sm text-muted-foreground"
      >
        Show auto-connected ({count})
      </Label>
    </div>
  );
}

export function KeysPage() {
  const search: { tab?: string } = useSearch({ strict: false });
  const navigate = useNavigate();
  const rawTab = search.tab ?? "services";
  const tab: TabValue = rawTab === "nyxid" ? "nyxid" : "services";

  const [addServiceOpen, setAddServiceOpen] = useState(false);
  const [showAutoConnected, setShowAutoConnected] = useState(false);

  const { data: keys } = useKeys();
  const autoCount = (keys ?? []).filter((k) => k.auto_connected).length;

  function setTab(value: string) {
    void navigate({ to: "/keys", search: { tab: value }, replace: true });
  }

  return (
    <div className="space-y-8">
      <PageHeader
        title="AI Services"
        description="Manage your AI service credentials and NyxID API keys."
        actions={
          <div className="flex items-center gap-4">
            {tab === "services" && (
              <AutoConnectedToggle
                checked={showAutoConnected}
                onCheckedChange={setShowAutoConnected}
                count={autoCount}
              />
            )}
            <AddButton
              tab={tab}
              onAddService={() => setAddServiceOpen(true)}
            />
          </div>
        }
      />

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="services">External Services</TabsTrigger>
          <TabsTrigger value="nyxid">NyxID API Keys</TabsTrigger>
        </TabsList>

        <TabsContent value="services" className="mt-6">
          <ExternalServicesTab
            onAdd={() => setAddServiceOpen(true)}
            showAutoConnected={showAutoConnected}
          />
        </TabsContent>

        <TabsContent value="nyxid" className="mt-6">
          <NyxIdApiKeysTab />
        </TabsContent>
      </Tabs>

      <AddKeyDialog open={addServiceOpen} onOpenChange={setAddServiceOpen} />
    </div>
  );
}
