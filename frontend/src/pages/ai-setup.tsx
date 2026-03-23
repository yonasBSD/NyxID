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
import { Copy, ExternalLink, Plus } from "lucide-react";
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
          Works with ChatGPT, Claude, Gemini, or any AI that can read URLs.
          Just say: <em>"Read {shortUrl} and help me integrate NyxID."</em>
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

      <Separator />

      {clients.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle className="text-base">MCP Config Generator</CardTitle>
              <CardDescription>
                Select your OAuth client and AI tool to generate a ready-to-paste
                configuration file.
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
