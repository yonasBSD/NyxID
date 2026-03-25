export type AiTool = "cursor" | "claude-code" | "codex" | "chatgpt";

export interface AiToolMeta {
  readonly id: AiTool;
  readonly name: string;
  readonly description: string;
  readonly configFileName: string;
  readonly configFilePath: string;
}

export const AI_TOOLS: readonly AiToolMeta[] = [
  {
    id: "cursor",
    name: "Cursor",
    description: "Native MCP support via project config",
    configFileName: "mcp.json",
    configFilePath: ".cursor/mcp.json",
  },
  {
    id: "claude-code",
    name: "Claude Code",
    description: "MCP via npx proxy command",
    configFileName: "settings.json",
    configFilePath: "~/.claude/settings.json",
  },
  {
    id: "codex",
    name: "Codex",
    description: "MCP via TOML config",
    configFileName: "config.toml",
    configFilePath: "~/.codex/config.toml",
  },
  {
    id: "chatgpt",
    name: "ChatGPT / Other",
    description: "Paste the llms.txt URL into your AI chat",
    configFileName: "llms.txt URL",
    configFilePath: "",
  },
] as const;

export interface AiToolConfigParams {
  readonly baseUrl: string;
  readonly mcpUrl: string;
  readonly clientId: string;
  readonly clientName: string;
  readonly redirectUris: readonly string[];
}

function cursorConfig(params: AiToolConfigParams): string {
  const config = {
    mcpServers: {
      nyxid: {
        url: params.mcpUrl,
      },
    },
  };
  return JSON.stringify(config, null, 2);
}

function claudeCodeConfig(params: AiToolConfigParams): string {
  const config = {
    mcpServers: {
      nyxid: {
        command: "npx",
        args: ["-y", "@anthropic-ai/mcp-proxy", params.mcpUrl],
        description: "NyxID MCP Proxy",
      },
    },
  };
  return JSON.stringify(config, null, 2);
}

function codexConfig(params: AiToolConfigParams): string {
  return `[mcp_servers.nyxid]\nurl = "${params.mcpUrl}"`;
}

function chatgptConfig(params: AiToolConfigParams): string {
  return [
    `# NyxID Integration Context`,
    ``,
    `Paste this message (or the URL below) into your AI chat to give it full context:`,
    ``,
    `  ${params.baseUrl}/llms-full.txt`,
    ``,
    `Or paste the short version:`,
    ``,
    `  ${params.baseUrl}/llms.txt`,
    ``,
    `## Your OAuth Client`,
    ``,
    `- Server: ${params.baseUrl}`,
    `- Client ID: ${params.clientId}`,
    `- Client Name: ${params.clientName}`,
    `- OIDC Discovery: ${params.baseUrl}/.well-known/openid-configuration`,
    ...(params.redirectUris.length > 0
      ? [`- Redirect URIs: ${params.redirectUris.join(", ")}`]
      : []),
    ``,
    `## Quick prompt`,
    ``,
    `> Read ${params.baseUrl}/llms-full.txt and help me integrate NyxID login`,
    `> into my app. My client_id is ${params.clientId}.`,
  ].join("\n");
}

const GENERATORS: Record<AiTool, (params: AiToolConfigParams) => string> = {
  cursor: cursorConfig,
  "claude-code": claudeCodeConfig,
  codex: codexConfig,
  chatgpt: chatgptConfig,
};

export function generateToolConfig(
  tool: AiTool,
  params: AiToolConfigParams,
): string {
  return GENERATORS[tool](params);
}
