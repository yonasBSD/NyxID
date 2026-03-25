import { useState, useMemo, useCallback } from "react";
import { Link } from "@tanstack/react-router";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";
import { toast } from "sonner";
import { Copy, ExternalLink, Plus, ArrowRight } from "lucide-react";
import { useDeveloperApps } from "@/hooks/use-developer-apps";
import { usePublicConfig } from "@/hooks/use-public-config";
import {
  AI_TOOLS,
  generateToolConfig,
  type AiTool,
  type AiToolConfigParams,
} from "@/lib/ai-tool-configs";

function CodeBlock({
  code,
  label,
  onCopy,
}: {
  readonly code: string;
  readonly label: string;
  readonly onCopy: () => void;
}) {
  return (
    <div className="relative">
      <div className="mb-2 flex items-center justify-between">
        <Badge variant="outline" className="text-[10px]">
          {label}
        </Badge>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1.5 text-xs"
          onClick={onCopy}
        >
          <Copy className="h-3 w-3" />
          Copy
        </Button>
      </div>
      <pre className="overflow-x-auto rounded-lg border border-border bg-muted px-4 py-3 font-mono text-xs leading-relaxed text-foreground">
        {code}
      </pre>
    </div>
  );
}

function EmptyState() {
  return (
    <Card>
      <CardContent className="flex flex-col items-center gap-4 py-12">
        <p className="text-sm text-muted-foreground">
          Create an OAuth client first to generate AI setup configs.
        </p>
        <Button asChild variant="outline" size="sm">
          <Link to="/developer/apps">
            <Plus className="mr-2 h-4 w-4" />
            Create Developer App
          </Link>
        </Button>
      </CardContent>
    </Card>
  );
}

interface DashboardLink {
  readonly to: string;
  readonly label: string;
}

interface QuickPrompt {
  readonly title: string;
  readonly description: string;
  readonly prompt: string;
  readonly links: readonly DashboardLink[];
}

function buildPreamble(baseUrl: string): string {
  return `I have the nyxid CLI installed. If I'm not logged in yet, run \`nyxid login --base-url ${baseUrl}\` to authenticate via browser SSO (this saves the URL for all future commands). Use the nyxid CLI for all operations (e.g., nyxid service add, nyxid catalog list, nyxid api-key create). For any secret (API keys, credentials, tokens), always use \`--credential-env VAR_NAME\` to read from environment variables -- NEVER ask me to paste secrets into chat or pass them as raw arguments. Use \`--output json\` for machine-readable output. Node commands accept names (e.g., nyxid node show my-server). `;
}

function buildQuickPrompts(baseUrl: string): readonly QuickPrompt[] {
  return [
    {
      title: "Add a key and connect credentials",
      description:
        "Add an external API key from the catalog or create a custom endpoint. Use nyxid service add or the AI Services page.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then help me add a new key in NyxID. I want to connect to an external API. Use \`nyxid catalog list --output json\` to browse available services, then \`nyxid service add <slug> --credential-env <VAR>\` to add one non-interactively. Alternatively, walk me through the AI Services page at /keys.`,
      links: [{ to: "/keys", label: "AI Services" }],
    },
    {
      title: "Set up MCP proxy for AI clients",
      description:
        "Configure Cursor, Claude Code, or Codex to use NyxID as an MCP proxy. Use nyxid mcp setup for auto-configuration.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then help me set up the NyxID MCP proxy in my AI coding tool so I can call APIs through it. If the nyxid CLI is available, use \`nyxid mcp setup cursor\` (or claude/codex) to auto-generate the config file.`,
      links: [],
    },
    {
      title: "Install and configure a node agent",
      description:
        "Deploy an on-premise node agent that keeps credentials on your infrastructure. Use nyxid node and nyxid-node CLIs.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then walk me through installing the nyxid-node agent, registering it, adding credentials, and routing services through it. Use \`nyxid node register-token\` to create a registration token, then \`nyxid-node register\` and \`nyxid-node credentials add\` on the target machine.`,
      links: [{ to: "/nodes", label: "Nodes" }],
    },
    {
      title: "Connect an OAuth or device-code service",
      description:
        "Add a service that uses OAuth, device code, or API key authentication. Use nyxid service add --oauth or the AI Services page.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then help me add a service that requires OAuth or device code authentication in NyxID. Use \`nyxid catalog list --output json\` to find the service, then \`nyxid service add <slug> --oauth\` or \`nyxid service add <slug> --device-code\` to start the auth flow. For client credentials, use \`--client-id-env\` and \`--client-secret-env\` flags. Alternatively, walk me through the AI Services page at /keys.`,
      links: [{ to: "/keys", label: "AI Services" }],
    },
    {
      title: "Set up approvals and notifications",
      description:
        "Require approval before accessing sensitive services. Get notified via Telegram or push notifications when approval is needed.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then help me set up approval workflows for my NyxID services. I want to configure which services require approval, set up Telegram notifications for approval requests, and understand how to approve or deny requests.`,
      links: [
        { to: "/approvals/settings", label: "Notifications" },
        { to: "/approvals/history", label: "Approvals" },
      ],
    },
    {
      title: "Set up SSH services",
      description:
        "Register an SSH service with certificate-based auth. Use nyxid ssh exec and nyxid ssh terminal for remote access.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then help me register an SSH service in NyxID with certificate-based authentication, and show me how to issue certificates, execute remote commands (via \`nyxid ssh exec\`), and open interactive terminals (via \`nyxid ssh terminal\`).`,
      links: [{ to: "/keys", label: "AI Services" }],
    },
    {
      title: "Make proxy requests",
      description:
        "Proxy API requests through NyxID with automatic credential injection. Use nyxid proxy request or curl.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then help me make proxy requests through NyxID. Use \`nyxid proxy discover --output json\` to list available services, then \`nyxid proxy request <slug> <path> -m POST -d '{...}'\` to send requests. Show me how to proxy LLM calls, use streaming, and route through nodes.`,
      links: [{ to: "/keys", label: "AI Services" }],
    },
    {
      title: "Connect OpenClaw AI gateway",
      description:
        "Connect a self-hosted OpenClaw instance to NyxID. Use nyxid openclaw connect or nyxid-node openclaw connect.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then help me connect my self-hosted OpenClaw AI gateway to NyxID. I want to:\n1. Connect OpenClaw via \`nyxid openclaw setup --url http://localhost:18789 --credential-env OPENCLAW_TOKEN\` or \`nyxid service add llm-openclaw --credential-env OPENCLAW_TOKEN\`\n2. Set up a node agent to proxy requests through my local OpenClaw instance (\`nyxid-node openclaw connect\`)\n3. Optionally set up channel integration so NyxID can interact with OpenClaw messaging channels (WhatsApp, Telegram, Discord, etc.)\n\nMy OpenClaw gateway is running at http://localhost:18789. Walk me through the fastest setup path -- ideally using the CLI with non-interactive flags.`,
      links: [
        { to: "/keys", label: "AI Services" },
        { to: "/nodes", label: "Nodes" },
      ],
    },
    {
      title: "Manage profile and security",
      description:
        "Update your profile, enable MFA, manage sessions, and review OAuth consents.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then help me secure my NyxID account. Use \`nyxid mfa setup\` to enable multi-factor authentication, \`nyxid session list\` to review active sessions, and \`nyxid profile consents\` to review OAuth consents. Also show me how to update my profile with \`nyxid profile update\`.`,
      links: [],
    },
    {
      title: "Add login to my app",
      description:
        "Register an OAuth client and integrate NyxID login into a React, Next.js, or any web app.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then help me add "Sign in with NyxID" to my app. The NyxID server is at ${baseUrl}.`,
      links: [{ to: "/developer/apps", label: "Developer Apps" }],
    },
    {
      title: "Create and manage API keys",
      description:
        "Create API keys for programmatic access. Use nyxid api-key create or the AI Services page.",
      prompt: `${buildPreamble(baseUrl)}Read ${baseUrl}/llms-full.txt then help me create and manage NyxID API keys for programmatic access. Use \`nyxid api-key create --name "My Key" --scopes "read write"\` or walk me through the AI Services page at /keys (NyxID API Keys tab).`,
      links: [{ to: "/keys", label: "AI Services" }],
    },
  ] as const;
}

function QuickPromptsCard({ baseUrl }: { readonly baseUrl: string }) {
  const prompts = useMemo(() => buildQuickPrompts(baseUrl), [baseUrl]);

  const handleCopy = useCallback(async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success("Copied prompt to clipboard");
    } catch {
      toast.error("Failed to copy");
    }
  }, []);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Quick Start Prompts</CardTitle>
        <CardDescription>
          Copy a prompt and paste it into your AI assistant. The prompts use the
          nyxid CLI which handles authentication securely.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="rounded-lg border border-border bg-muted/50 p-4 space-y-3">
          <p className="text-sm font-medium">Before you start</p>
          <p className="text-xs text-muted-foreground">
            Install the NyxID CLI and log in:
          </p>
          <div className="relative">
            <pre className="overflow-x-auto rounded-md border border-border bg-muted px-3 py-2 font-mono text-xs">
              {`cargo install --git https://github.com/ChronoAIProject/NyxID --bin nyxid\nnyxid login --base-url ${baseUrl}`}
            </pre>
            <Button
              variant="ghost"
              size="icon"
              className="absolute right-1 top-1 h-6 w-6"
              onClick={() =>
                void handleCopy(
                  `cargo install --git https://github.com/ChronoAIProject/NyxID --bin nyxid\nnyxid login --base-url ${baseUrl}`,
                )
              }
            >
              <Copy className="h-3 w-3" />
            </Button>
          </div>
          <p className="text-[11px] text-muted-foreground">
            The CLI authenticates via browser SSO. Copy a prompt below and paste
            it into your AI assistant. The CLI handles secrets securely -- your
            AI never needs to see API keys or tokens.
          </p>
        </div>
        {prompts.map((p) => (
          <div
            key={p.title}
            className="rounded-lg border border-border p-3 space-y-2"
          >
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0">
                <p className="text-sm font-medium">{p.title}</p>
                <p className="text-xs text-muted-foreground">{p.description}</p>
              </div>
              <div className="flex shrink-0 gap-1">
                {p.links.map((link) => (
                  <Button
                    key={link.to}
                    variant="ghost"
                    size="sm"
                    className="h-7 gap-1 text-xs"
                    asChild
                  >
                    <Link to={link.to}>
                      {link.label}
                      <ArrowRight className="h-3 w-3" />
                    </Link>
                  </Button>
                ))}
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 gap-1 text-xs"
                  onClick={() => void handleCopy(p.prompt)}
                >
                  <Copy className="h-3 w-3" />
                  Copy prompt
                </Button>
              </div>
            </div>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}

function LlmsTxtCard({ baseUrl }: { readonly baseUrl: string }) {
  const shortUrl = `${baseUrl}/llms.txt`;
  const fullUrl = `${baseUrl}/llms-full.txt`;

  const handleCopy = useCallback(async (url: string) => {
    try {
      await navigator.clipboard.writeText(url);
      toast.success("Copied to clipboard");
    } catch {
      toast.error("Failed to copy");
    }
  }, []);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Direct URL for Any AI</CardTitle>
        <CardDescription>
          Tell your AI agent to read one of these URLs for full NyxID context
          with your deployment's actual endpoints.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-2 rounded-lg border border-border bg-muted px-3 py-2">
          <code className="flex-1 truncate font-mono text-xs">{shortUrl}</code>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0"
            onClick={() => void handleCopy(shortUrl)}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0"
            asChild
          >
            <a href={shortUrl} target="_blank" rel="noopener noreferrer">
              <ExternalLink className="h-3.5 w-3.5" />
            </a>
          </Button>
        </div>
        <div className="flex items-center gap-2 rounded-lg border border-border bg-muted px-3 py-2">
          <code className="flex-1 truncate font-mono text-xs">{fullUrl}</code>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0"
            onClick={() => void handleCopy(fullUrl)}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0"
            asChild
          >
            <a href={fullUrl} target="_blank" rel="noopener noreferrer">
              <ExternalLink className="h-3.5 w-3.5" />
            </a>
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          Works with ChatGPT, Claude, Gemini, or any AI that can read URLs. Just
          say: <em>"Read {shortUrl} and help me integrate NyxID."</em>
        </p>
      </CardContent>
    </Card>
  );
}

export function AiSetupPage() {
  const { data: appsData, isLoading: appsLoading } = useDeveloperApps();
  const { data: config, isLoading: configLoading } = usePublicConfig();

  const [selectedClientId, setSelectedClientId] = useState<string>("");
  const [selectedTool, setSelectedTool] = useState<AiTool>("cursor");

  const clients = appsData?.clients ?? [];
  const mcpUrl = config?.mcp_url ?? `${window.location.origin}/mcp`;
  const baseUrl = mcpUrl.replace(/\/mcp$/, "");

  const selectedClient = useMemo(
    () => clients.find((c) => c.id === selectedClientId) ?? clients[0] ?? null,
    [clients, selectedClientId],
  );

  const configParams: AiToolConfigParams | null = useMemo(() => {
    if (!selectedClient) return null;
    return {
      baseUrl,
      mcpUrl,
      clientId: selectedClient.id,
      clientName: selectedClient.client_name,
      redirectUris: selectedClient.redirect_uris,
    };
  }, [baseUrl, mcpUrl, selectedClient]);

  const generatedConfig = useMemo(() => {
    if (!configParams) return "";
    return generateToolConfig(selectedTool, configParams);
  }, [selectedTool, configParams]);

  const toolMeta = AI_TOOLS.find((t) => t.id === selectedTool)!;

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(generatedConfig);
      toast.success("Copied to clipboard");
    } catch {
      toast.error("Failed to copy to clipboard");
    }
  }, [generatedConfig]);

  if (appsLoading || configLoading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-10 w-48" />
        <Skeleton className="h-32 w-full" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  return (
    <div className="space-y-8">
      <div>
        <h2 className="font-display text-3xl font-normal tracking-tight md:text-5xl">
          AI Setup
        </h2>
        <p className="text-muted-foreground">
          Configure your AI coding assistant to work with NyxID. Pick a tool and
          an app, copy the config, done.
        </p>
      </div>

      <LlmsTxtCard baseUrl={baseUrl} />

      <QuickPromptsCard baseUrl={baseUrl} />

      <Separator />

      {clients.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle className="text-base">MCP Config Generator</CardTitle>
              <CardDescription>
                Select your OAuth client and AI tool to generate a
                ready-to-paste configuration file.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              {/* App selector */}
              <div className="space-y-2">
                <label className="text-sm font-medium">OAuth Client</label>
                <Select
                  value={selectedClient?.id ?? ""}
                  onValueChange={setSelectedClientId}
                >
                  <SelectTrigger className="w-full max-w-md">
                    <SelectValue placeholder="Select an app" />
                  </SelectTrigger>
                  <SelectContent>
                    {clients.map((client) => (
                      <SelectItem key={client.id} value={client.id}>
                        <span className="flex items-center gap-2">
                          {client.client_name}
                          <Badge variant="outline" className="text-[10px]">
                            {client.client_type}
                          </Badge>
                        </span>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {/* Tool tabs */}
              <Tabs
                value={selectedTool}
                onValueChange={(v) => setSelectedTool(v as AiTool)}
                className="space-y-4"
              >
                <TabsList>
                  {AI_TOOLS.map((tool) => (
                    <TabsTrigger key={tool.id} value={tool.id}>
                      {tool.name}
                    </TabsTrigger>
                  ))}
                </TabsList>

                {AI_TOOLS.map((tool) => (
                  <TabsContent key={tool.id} value={tool.id}>
                    <div className="space-y-4">
                      {tool.configFilePath && (
                        <p className="text-sm text-muted-foreground">
                          Save to{" "}
                          <code className="rounded bg-muted px-1.5 py-0.5 text-xs">
                            {tool.configFilePath}
                          </code>
                          {tool.id === "claude-code" && (
                            <span>
                              {" "}
                              or project-level{" "}
                              <code className="rounded bg-muted px-1.5 py-0.5 text-xs">
                                .claude/settings.json
                              </code>
                            </span>
                          )}
                        </p>
                      )}
                      <CodeBlock
                        code={generatedConfig}
                        label={toolMeta.configFileName}
                        onCopy={handleCopy}
                      />
                    </div>
                  </TabsContent>
                ))}
              </Tabs>
            </CardContent>
          </Card>

          {/* Context for the selected client */}
          {selectedClient && (
            <Card>
              <CardHeader>
                <CardTitle className="text-base">Client Details</CardTitle>
              </CardHeader>
              <CardContent>
                <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm">
                  <dt className="text-muted-foreground">Client ID</dt>
                  <dd className="font-mono text-xs">{selectedClient.id}</dd>
                  <dt className="text-muted-foreground">Type</dt>
                  <dd>{selectedClient.client_type}</dd>
                  <dt className="text-muted-foreground">Redirect URIs</dt>
                  <dd className="font-mono text-xs">
                    {selectedClient.redirect_uris.join(", ") || "None"}
                  </dd>
                  <dt className="text-muted-foreground">Scopes</dt>
                  <dd className="font-mono text-xs">
                    {selectedClient.allowed_scopes || "openid profile email"}
                  </dd>
                </dl>
              </CardContent>
            </Card>
          )}
        </div>
      )}
    </div>
  );
}
