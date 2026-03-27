## Recommended: Set up approval notifications

Before using NyxID with AI agents, set up a notification channel so you can approve
or deny service access requests in real time:

**Link Telegram** (fastest to set up):
```
nyxid notification telegram-link
```

**Download the NyxID mobile app** (approve from anywhere):
- https://nyxid.onelink.me/REzJ/dql9w8fx (auto-detects iOS or Android)

Approval protection is enabled automatically when you link Telegram or register a
device. You can also ask your AI agent: "Set up my NyxID notifications"

## Important: Activating the skill

Start a new chat in OpenClaw to load the NyxID skill. Do **not** run
`openclaw gateway restart` unless the gateway is installed as a system service
(e.g. via systemd or launchd). Restarting a manually-started gateway will stop
it and it will not come back up automatically.

## What you can do now

Try asking your AI agent any of these:

- "Set up my NyxID notifications" -- link Telegram or configure push notifications
- "Connect my OpenAI account to NyxID" -- walks you through adding credentials
- "What services do I have in NyxID?" -- lists your connected services
- "Call the OpenAI API through NyxID" -- proxies requests with your stored credentials
- "Add my Anthropic API key" -- guided setup with link to the provider portal
- "Set up a new credential node" -- deploy an on-premise credential agent
- "Show my NyxID account status" -- overview of services, keys, and nodes
- "Create an API key for my app" -- programmatic access to NyxID
- "Set up MCP for Cursor" -- generate MCP config for any AI tool
- "SSH into my-server" -- remote access through NyxID
- "Browse the service catalog" -- see all available services you can connect

The agent handles everything through the `nyxid` CLI. Your credentials are stored
securely in NyxID and never exposed to the agent.

To update the skill with the latest capabilities:
  nyxid ai-setup update
