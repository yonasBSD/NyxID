import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "@tanstack/react-router";
import { useAllAdminedApiKeys, useCreateApiKey, useUpdateApiKey } from "@/hooks/use-api-keys";
import { useAgentBindings, useCreateBinding } from "@/hooks/use-agent-bindings";
import { useKeys } from "@/hooks/use-keys";
import { useOrgs } from "@/hooks/use-orgs";
import { ApiError } from "@/lib/api-client";
import { cn, copyToClipboard } from "@/lib/utils";
import type { ApiKey } from "@/types/api";
import type { CreateApiKeyFormData } from "@/schemas/api-keys";
import type { CredentialSource } from "@/schemas/orgs";
import { Badge } from "@/components/ui/badge";
import { Button, ButtonIcon } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Check,
  CheckCircle2,
  Copy,
  KeyRound,
  Link2,
  PlayCircle,
  Shield,
  Terminal,
} from "lucide-react";
import { toast } from "sonner";

const AGENT_PLATFORM_OPTIONS = [
  { id: "claude-code", label: "Claude Code" },
  { id: "cursor", label: "Cursor" },
  { id: "codex", label: "Codex" },
  { id: "openclaw", label: "OpenClaw" },
  { id: "generic", label: "Generic Agent" },
] as const;

type AgentPlatform = (typeof AGENT_PLATFORM_OPTIONS)[number]["id"];
type KeyMode = "new" | "existing";
type DialogStep = "configure" | "complete";

interface SelectedServiceSummary {
  readonly id: string;
  readonly slug: string;
  readonly label: string;
  readonly hasCredential: boolean;
}

interface CompletionState {
  readonly keyId: string;
  readonly keyName: string;
  readonly fullKey: string | null;
  readonly services: readonly SelectedServiceSummary[];
  readonly bindingsCreated: number;
  readonly bindingsSkipped: number;
}

function isAgentPlatform(value: string | null | undefined): value is AgentPlatform {
  return AGENT_PLATFORM_OPTIONS.some((option) => option.id === value);
}

function platformLabel(platform: AgentPlatform): string {
  return AGENT_PLATFORM_OPTIONS.find((option) => option.id === platform)?.label ?? platform;
}

function defaultKeyName(platform: AgentPlatform): string {
  return `${platformLabel(platform)} Agent`;
}

function parseScopesString(scopes: string): readonly string[] {
  if (!scopes.trim()) return [];
  return scopes.trim().split(/\s+/);
}

function ensureAgentScopes(scopes: string): string {
  const next = new Set(parseScopesString(scopes));
  next.add("proxy");
  next.add("services:read");
  return Array.from(next).join(" ");
}

function sameOwner(a?: CredentialSource, b?: CredentialSource): boolean {
  const aType = a?.type ?? "personal";
  const bType = b?.type ?? "personal";
  if (aType !== bType) return false;
  if (aType === "personal") return true;
  return a?.type === "org" && b?.type === "org" && a.org_id === b.org_id;
}

function serviceOwnerLabel(source?: CredentialSource): string {
  if (source?.type === "org") return source.org_name;
  return "Personal";
}

function apiKeyOwnerLabel(key: ApiKey): string {
  const source = key.credential_source;
  if (source?.type === "org") return source.org_name;
  return "Personal";
}

function shellQuote(value: string): string {
  return `'${value.replace(/'/g, "'\\''")}'`;
}

function buildConfigSnippet(completion: CompletionState): string {
  const keyValue = completion.fullKey
    ? shellQuote(completion.fullKey)
    : "<paste-agent-key>";
  const serviceList = completion.services.map((service) => service.slug).join(",");
  return [
    `export NYXID_BASE_URL=${shellQuote(`${window.location.origin}/api/v1`)}`,
    `export NYXID_AGENT_KEY=${keyValue}`,
    `export NYXID_ALLOWED_SERVICES=${shellQuote(serviceList)}`,
  ].join("\n");
}

function buildVerificationCommand(
  completion: CompletionState,
  deniedService: SelectedServiceSummary | null,
): string {
  const allowedService = completion.services[0];
  if (!allowedService) return "";

  const keyValue = completion.fullKey
    ? shellQuote(completion.fullKey)
    : "<paste-agent-key>";
  const base = `${window.location.origin}/api/v1/proxy/s`;
  const lines = [
    `export NYXID_AGENT_KEY=${keyValue}`,
    "",
    `curl -sS -o /dev/null -w "allowed ${allowedService.slug} HTTP=%{http_code}\\n" \\`,
    `  -H "X-API-Key: $NYXID_AGENT_KEY" \\`,
    `  ${shellQuote(`${base}/${allowedService.slug}`)}`,
  ];

  if (deniedService) {
    lines.push(
      "",
      `curl -sS -o /dev/null -w "denied ${deniedService.slug} HTTP=%{http_code}\\n" \\`,
      `  -H "X-API-Key: $NYXID_AGENT_KEY" \\`,
      `  ${shellQuote(`${base}/${deniedService.slug}`)}`,
    );
  }

  return lines.join("\n");
}

function CodeBlock({
  label,
  code,
}: {
  readonly label: string;
  readonly code: string;
}) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    try {
      await copyToClipboard(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
      toast.success("Copied to clipboard");
    } catch {
      toast.error("Failed to copy");
    }
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <p className="text-[11px] font-medium uppercase tracking-wide text-text-tertiary">
          {label}
        </p>
        <Button variant="ghost" size="icon" className="h-7 w-7" onClick={() => void handleCopy()}>
          {copied ? (
            <Check className="h-3.5 w-3.5 text-success" aria-hidden="true" />
          ) : (
            <Copy className="h-3.5 w-3.5" aria-hidden="true" />
          )}
          <span className="sr-only">Copy {label}</span>
        </Button>
      </div>
      <pre className="whitespace-pre-wrap break-all rounded-lg border border-border bg-muted px-4 py-3.5 font-mono text-xs leading-relaxed">
        {code}
      </pre>
    </div>
  );
}

export function AgentIsolationSetupDialog({
  open,
  onOpenChange,
  initialServiceId,
  initialApiKeyId,
}: {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly initialServiceId?: string | null;
  readonly initialApiKeyId?: string | null;
}) {
  const { data: services } = useKeys();
  const { data: apiKeys } = useAllAdminedApiKeys();
  const { data: orgs } = useOrgs();
  const createApiKey = useCreateApiKey();
  const updateApiKey = useUpdateApiKey();
  const createBinding = useCreateBinding();

  const [step, setStep] = useState<DialogStep>("configure");
  const [platform, setPlatform] = useState<AgentPlatform>("claude-code");
  const [keyMode, setKeyMode] = useState<KeyMode>("new");
  const [keyName, setKeyName] = useState(defaultKeyName("claude-code"));
  const [selectedKeyId, setSelectedKeyId] = useState("");
  const [targetOrgId, setTargetOrgId] = useState<string | null>(null);
  const [selectedServiceIds, setSelectedServiceIds] = useState<readonly string[]>([]);
  const [bindCredentials, setBindCredentials] = useState(true);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [completion, setCompletion] = useState<CompletionState | null>(null);
  const initializedRef = useRef(false);

  const existingBindingKeyId = keyMode === "existing" ? selectedKeyId : "";
  const { data: existingBindings, isLoading: bindingsLoading } =
    useAgentBindings(existingBindingKeyId);

  const adminOrgs = useMemo(
    () => (orgs ?? []).filter((org) => org.your_role === "admin"),
    [orgs],
  );

  const selectedApiKey = useMemo(
    () => (apiKeys ?? []).find((key) => key.id === selectedKeyId) ?? null,
    [apiKeys, selectedKeyId],
  );

  const ownerSource = useMemo<CredentialSource | undefined>(() => {
    if (keyMode === "existing") return selectedApiKey?.credential_source;
    if (!targetOrgId) return { type: "personal" };
    const org = adminOrgs.find((item) => item.id === targetOrgId);
    return {
      type: "org",
      org_id: targetOrgId,
      org_name: org?.display_name ?? org?.slug ?? "Organization",
      avatar_url: org?.avatar_url ?? null,
      role: "admin",
      allowed: true,
    };
  }, [adminOrgs, keyMode, selectedApiKey, targetOrgId]);

  const availableServices = useMemo(
    () =>
      (services ?? []).filter(
        (service) =>
          service.is_active &&
          sameOwner(service.credential_source, ownerSource),
      ),
    [ownerSource, services],
  );

  const selectedServices = useMemo(
    () =>
      availableServices.filter((service) =>
        selectedServiceIds.includes(service.id),
      ),
    [availableServices, selectedServiceIds],
  );

  const boundServiceIds = useMemo(
    () => new Set((existingBindings ?? []).map((binding) => binding.user_service_id)),
    [existingBindings],
  );

  const bindableSelectedServices = useMemo(
    () =>
      selectedServices.filter(
        (service) =>
          Boolean(service.api_key_id) &&
          (keyMode === "new" || !boundServiceIds.has(service.id)),
      ),
    [boundServiceIds, keyMode, selectedServices],
  );

  const deniedService = useMemo<SelectedServiceSummary | null>(() => {
    if (!completion) return null;
    const allowedIds = new Set(completion.services.map((service) => service.id));
    const candidate = availableServices.find((service) => !allowedIds.has(service.id));
    return candidate
      ? {
          id: candidate.id,
          slug: candidate.slug,
          label: candidate.label,
          hasCredential: Boolean(candidate.api_key_id),
        }
      : null;
  }, [availableServices, completion]);

  useEffect(() => {
    if (!open) {
      initializedRef.current = false;
      return;
    }
    if (initializedRef.current) return;
    if (initialServiceId && services === undefined) return;
    if (initialApiKeyId && apiKeys === undefined) return;
    const pendingInitialService = (services ?? []).find((service) => service.id === initialServiceId);
    if (pendingInitialService?.credential_source?.type === "org" && orgs === undefined) {
      return;
    }

    initializedRef.current = true;
    const initialKey = (apiKeys ?? []).find((key) => key.id === initialApiKeyId);
    const initialService = (services ?? []).find((service) => service.id === initialServiceId);
    const initialPlatform = isAgentPlatform(initialKey?.platform)
      ? initialKey.platform
      : "claude-code";

    setStep("configure");
    setCompletion(null);
    setSubmitError(null);
    setPlatform(initialPlatform);
    setKeyName(defaultKeyName(initialPlatform));
    setBindCredentials(true);

    if (initialKey) {
      setKeyMode("existing");
      setSelectedKeyId(initialKey.id);
    } else {
      setKeyMode("new");
      setSelectedKeyId("");
    }

    const initialSource = initialService?.credential_source;
    if (
      initialSource?.type === "org" &&
      adminOrgs.some((org) => org.id === initialSource.org_id)
    ) {
      setTargetOrgId(initialSource.org_id);
    } else {
      setTargetOrgId(null);
    }

    setSelectedServiceIds(initialService ? [initialService.id] : []);
  }, [adminOrgs, apiKeys, initialApiKeyId, initialServiceId, open, orgs, services]);

  function handlePlatformChange(value: string) {
    if (!isAgentPlatform(value)) return;
    setPlatform(value);
    if (keyMode === "new" && keyName === defaultKeyName(platform)) {
      setKeyName(defaultKeyName(value));
    }
  }

  function handleKeyModeChange(nextMode: KeyMode) {
    setKeyMode(nextMode);
    setSubmitError(null);
    const firstKey = apiKeys?.[0];
    if (nextMode === "existing" && !selectedKeyId && firstKey) {
      handleExistingKeyChange(firstKey.id);
    }
  }

  function handleExistingKeyChange(keyId: string) {
    setSelectedKeyId(keyId);
    const key = (apiKeys ?? []).find((item) => item.id === keyId);
    if (isAgentPlatform(key?.platform)) {
      setPlatform(key.platform);
    }
    setSelectedServiceIds((current) =>
      current.filter((serviceId) => {
        const service = (services ?? []).find((item) => item.id === serviceId);
        return service ? sameOwner(service.credential_source, key?.credential_source) : false;
      }),
    );
    setSubmitError(null);
  }

  function handleTargetOwnerChange(value: string) {
    const orgId = value === "personal" ? null : value.replace(/^org:/, "");
    setTargetOrgId(orgId);
    setSelectedServiceIds([]);
    setSubmitError(null);
  }

  function toggleService(serviceId: string) {
    setSelectedServiceIds((current) =>
      current.includes(serviceId)
        ? current.filter((id) => id !== serviceId)
        : [...current, serviceId],
    );
  }

  async function handleSubmit() {
    const serviceIds = selectedServices.map((service) => service.id);
    if (serviceIds.length === 0) {
      setSubmitError("Select at least one service for this agent.");
      return;
    }
    if (keyMode === "new" && !keyName.trim()) {
      setSubmitError("Name the new Agent Key.");
      return;
    }
    if (keyMode === "existing" && !selectedApiKey) {
      setSubmitError("Select an existing Agent Key.");
      return;
    }

    setSubmitError(null);
    try {
      let keyId: string;
      let fullKey: string | null = null;
      let name: string;

      if (keyMode === "new") {
        const payload: CreateApiKeyFormData = {
          name: keyName.trim(),
          scopes: ["proxy", "services:read"],
          expires_at: null,
          description: `Isolated Agent Key for ${platformLabel(platform)}`,
          allow_all_services: false,
          allowed_service_ids: serviceIds,
          allow_all_nodes: true,
          allowed_node_ids: [],
          callback_url: null,
          platform,
          target_org_id: targetOrgId ?? undefined,
        };
        const created = await createApiKey.mutateAsync(payload);
        keyId = created.id;
        fullKey = created.full_key;
        name = created.name;
      } else {
        const key = selectedApiKey!;
        const updated = await updateApiKey.mutateAsync({
          keyId: key.id,
          scopes: ensureAgentScopes(key.scopes),
          platform,
          allow_all_services: false,
          allowed_service_ids: serviceIds,
        });
        keyId = updated.id;
        name = updated.name;
      }

      let bindingsCreated = 0;
      let bindingsSkipped = selectedServices.length - bindableSelectedServices.length;
      if (bindCredentials && bindableSelectedServices.length > 0) {
        for (const service of bindableSelectedServices) {
          if (!service.api_key_id) continue;
          await createBinding.mutateAsync({
            keyId,
            user_service_id: service.id,
            user_api_key_id: service.api_key_id,
          });
          bindingsCreated += 1;
        }
      } else {
        bindingsSkipped = selectedServices.length;
      }

      setCompletion({
        keyId,
        keyName: name,
        fullKey,
        services: selectedServices.map((service) => ({
          id: service.id,
          slug: service.slug,
          label: service.label,
          hasCredential: Boolean(service.api_key_id),
        })),
        bindingsCreated,
        bindingsSkipped,
      });
      setStep("complete");
      toast.success("Agent isolation setup complete");
    } catch (error) {
      const message =
        error instanceof ApiError
          ? error.message
          : "Failed to complete agent setup.";
      setSubmitError(message);
      toast.error(message);
    }
  }

  function handleClose(nextOpen: boolean) {
    onOpenChange(nextOpen);
  }

  const ownerSelectValue = targetOrgId ? `org:${targetOrgId}` : "personal";
  const canSubmit =
    selectedServices.length > 0 &&
    (keyMode === "new" ? keyName.trim().length > 0 : Boolean(selectedApiKey)) &&
    !(
      keyMode === "existing" &&
      bindCredentials &&
      Boolean(selectedKeyId) &&
      bindingsLoading
    );
  const isSubmitting =
    createApiKey.isPending || updateApiKey.isPending || createBinding.isPending;

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="md:max-w-3xl">
        {step === "complete" && completion ? (
          <>
            <DialogHeader>
              <DialogTitle>Agent Isolation Complete</DialogTitle>
              <DialogDescription>
                The Agent Key is restricted to the selected service scope. Copy
                the key material now if a new key was created, then run the
                verification command before handing it to the agent.
              </DialogDescription>
            </DialogHeader>

            <div className="space-y-4">
              <div className="grid gap-3 sm:grid-cols-3">
                <div className="rounded-lg border border-border p-3">
                  <div className="flex items-center gap-2">
                    <CheckCircle2 className="h-3.5 w-3.5 text-success" />
                    <p className="text-[12px] font-medium">Key</p>
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {completion.keyName}
                  </p>
                </div>
                <div className="rounded-lg border border-border p-3">
                  <div className="flex items-center gap-2">
                    <Shield className="h-3.5 w-3.5 text-success" />
                    <p className="text-[12px] font-medium">Scope</p>
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {completion.services.length} service{completion.services.length === 1 ? "" : "s"}
                  </p>
                </div>
                <div className="rounded-lg border border-border p-3">
                  <div className="flex items-center gap-2">
                    <Link2 className="h-3.5 w-3.5 text-success" />
                    <p className="text-[12px] font-medium">Bindings</p>
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {completion.bindingsCreated > 0
                      ? `${completion.bindingsCreated} pinned`
                      : "Scope only"}
                  </p>
                </div>
              </div>

              {completion.fullKey && (
                <CodeBlock label="Agent Key" code={completion.fullKey} />
              )}

              <div className="rounded-lg border border-border bg-muted/20 p-3">
                <p className="text-[12px] font-medium">Allowed Services</p>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {completion.services.map((service) => (
                    <Badge key={service.id} variant="secondary">
                      {service.label} ({service.slug})
                    </Badge>
                  ))}
                </div>
                {completion.bindingsSkipped > 0 && (
                  <p className="mt-2 text-xs text-muted-foreground">
                    {completion.bindingsSkipped} service{completion.bindingsSkipped === 1 ? "" : "s"} use
                    scope-only access. Add bindings later only when this agent
                    must use an explicit credential override.
                  </p>
                )}
              </div>

              <CodeBlock label="Agent Config" code={buildConfigSnippet(completion)} />
              <CodeBlock
                label="Scope Verification"
                code={buildVerificationCommand(completion, deniedService)}
              />
              <p className="text-xs text-muted-foreground">
                The allowed check should not fail with a NyxID scope error. The
                denied check should return a forbidden response when another
                same-owner service exists.
              </p>
            </div>

            <DialogFooter>
              <Button
                variant="outline"
                asChild
                onClick={() => onOpenChange(false)}
              >
                <Link to="/keys/api-key/$keyId" params={{ keyId: completion.keyId }}>
                  Open Agent Key
                </Link>
              </Button>
              <Button variant="primary" onClick={() => onOpenChange(false)}>
                Done
              </Button>
            </DialogFooter>
          </>
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Set Up an AI Agent</DialogTitle>
              <DialogDescription>
                Create or select an Agent Key, restrict it to specific services,
                optionally pin credentials, then copy a verification command.
              </DialogDescription>
            </DialogHeader>

            <div className="space-y-5">
              <section className="space-y-3 rounded-lg border border-border p-4">
                <div className="flex items-center gap-2">
                  <Terminal className="h-4 w-4 text-primary" />
                  <div>
                    <p className="text-[12px] font-medium">1. Choose Target Tool</p>
                    <p className="text-xs text-muted-foreground">
                      The platform label is used for audit attribution and table
                      filtering.
                    </p>
                  </div>
                </div>
                <Select value={platform} onValueChange={handlePlatformChange}>
                  <SelectTrigger className="max-w-sm">
                    <SelectValue placeholder="Select platform" />
                  </SelectTrigger>
                  <SelectContent>
                    {AGENT_PLATFORM_OPTIONS.map((option) => (
                      <SelectItem key={option.id} value={option.id}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </section>

              <section className="space-y-3 rounded-lg border border-border p-4">
                <div className="flex items-center gap-2">
                  <KeyRound className="h-4 w-4 text-primary" />
                  <div>
                    <p className="text-[12px] font-medium">2. Agent Key</p>
                    <p className="text-xs text-muted-foreground">
                      New keys default to least-privilege proxy access for the
                      services selected below.
                    </p>
                  </div>
                </div>

                <div className="grid gap-2 sm:grid-cols-2">
                  <Button
                    type="button"
                    variant={keyMode === "new" ? "default" : "outline"}
                    className="justify-start"
                    onClick={() => handleKeyModeChange("new")}
                  >
                    <ButtonIcon><KeyRound className="h-3 w-3" /></ButtonIcon>
                    Create new Agent Key
                  </Button>
                  <Button
                    type="button"
                    variant={keyMode === "existing" ? "default" : "outline"}
                    className="justify-start"
                    disabled={!apiKeys || apiKeys.length === 0}
                    onClick={() => handleKeyModeChange("existing")}
                  >
                    <ButtonIcon><Shield className="h-3 w-3" /></ButtonIcon>
                    Use existing Agent Key
                  </Button>
                </div>

                {keyMode === "new" ? (
                  <div className="grid gap-3 sm:grid-cols-2">
                    <div className="space-y-1.5">
                      <Label htmlFor="agent-key-name" className="text-xs">
                        Key name
                      </Label>
                      <Input
                        id="agent-key-name"
                        value={keyName}
                        maxLength={64}
                        onChange={(event) => setKeyName(event.target.value)}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label className="text-xs">Owner</Label>
                      <Select value={ownerSelectValue} onValueChange={handleTargetOwnerChange}>
                        <SelectTrigger>
                          <SelectValue placeholder="Select owner" />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="personal">Personal</SelectItem>
                          {adminOrgs.map((org) => (
                            <SelectItem key={org.id} value={`org:${org.id}`}>
                              {org.display_name ?? org.slug}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </div>
                  </div>
                ) : (
                  <div className="space-y-1.5">
                    <Label className="text-xs">Existing key</Label>
                    <Select value={selectedKeyId} onValueChange={handleExistingKeyChange}>
                      <SelectTrigger>
                        <SelectValue placeholder="Select Agent Key" />
                      </SelectTrigger>
                      <SelectContent>
                        {(apiKeys ?? []).map((key) => (
                          <SelectItem key={key.id} value={key.id}>
                            {key.name} ({apiKeyOwnerLabel(key)})
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                )}
              </section>

              <section className="space-y-3 rounded-lg border border-border p-4">
                <div className="flex items-center gap-2">
                  <Shield className="h-4 w-4 text-primary" />
                  <div>
                    <p className="text-[12px] font-medium">3. Allowed Services</p>
                    <p className="text-xs text-muted-foreground">
                      Restrict the key to the services this agent should proxy.
                    </p>
                  </div>
                </div>

                {availableServices.length > 0 ? (
                  <div className="grid max-h-56 gap-2 overflow-y-auto pr-1 sm:grid-cols-2">
                    {availableServices.map((service) => {
                      const checked = selectedServiceIds.includes(service.id);
                      return (
                        <div
                          key={service.id}
                          role="button"
                          tabIndex={0}
                          onClick={() => toggleService(service.id)}
                          onKeyDown={(event) => {
                            if (event.key === "Enter" || event.key === " ") {
                              event.preventDefault();
                              toggleService(service.id);
                            }
                          }}
                          className={cn(
                            "flex min-h-[58px] cursor-pointer items-start gap-2 rounded-lg border p-3 text-left transition-colors",
                            checked
                              ? "border-primary/50 bg-primary/10"
                              : "border-border hover:border-white/[0.15] hover:bg-muted/30",
                          )}
                        >
                          <Checkbox
                            checked={checked}
                            onCheckedChange={() => toggleService(service.id)}
                            onClick={(event) => event.stopPropagation()}
                            onKeyDown={(event) => event.stopPropagation()}
                            aria-label={`Allow ${service.label}`}
                          />
                          <span className="min-w-0">
                            <span className="block truncate text-[12px] font-medium">
                              {service.label}
                            </span>
                            <span className="block truncate text-xs text-muted-foreground">
                              {service.slug} · {serviceOwnerLabel(service.credential_source)}
                            </span>
                          </span>
                        </div>
                      );
                    })}
                  </div>
                ) : (
                  <div className="rounded-lg border border-border bg-muted/20 p-3 text-xs text-muted-foreground">
                    No active services are available for this owner. Add a
                    service first, or choose an existing key with a different
                    owner.
                  </div>
                )}
              </section>

              <section className="space-y-3 rounded-lg border border-border p-4">
                <div className="flex items-center gap-2">
                  <Link2 className="h-4 w-4 text-primary" />
                  <div>
                    <p className="text-[12px] font-medium">4. Credential Binding</p>
                    <p className="text-xs text-muted-foreground">
                      Scope-only access is enough when the agent can use the
                      service default credential. Bind credentials when this
                      agent needs an explicit per-service override.
                    </p>
                  </div>
                </div>
                <div className="flex items-start gap-2 rounded-lg border border-border bg-muted/20 p-3">
                  <Checkbox
                    id="bind-agent-credentials"
                    checked={bindCredentials}
                    disabled={bindableSelectedServices.length === 0}
                    onCheckedChange={(checked) => setBindCredentials(checked === true)}
                  />
                  <Label htmlFor="bind-agent-credentials" className="space-y-1 text-xs">
                    <span className="block font-medium">
                      Pin selected service credentials to this Agent Key
                    </span>
                    <span className="block text-muted-foreground">
                      {bindableSelectedServices.length > 0
                        ? `${bindableSelectedServices.length} selected service${bindableSelectedServices.length === 1 ? "" : "s"} can be bound now.`
                        : "No selected credential-backed services need a new binding."}
                    </span>
                  </Label>
                </div>
              </section>

              {submitError && (
                <div className="rounded-lg border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive">
                  {submitError}
                </div>
              )}
            </div>

            <DialogFooter>
              <Button variant="outline" onClick={() => onOpenChange(false)}>
                Cancel
              </Button>
              <Button
                variant="primary"
                onClick={() => void handleSubmit()}
                disabled={!canSubmit}
                isLoading={isSubmitting}
              >
                <ButtonIcon variant="primary"><PlayCircle className="h-3 w-3" /></ButtonIcon>
                Complete Setup
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
