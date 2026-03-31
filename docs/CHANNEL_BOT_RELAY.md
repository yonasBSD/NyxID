# Channel Bot Relay Design

## Overview

NyxID Channel Bot Relay turns NyxID into a **multi-platform messaging gateway**. Users register their own bots (Telegram, Discord, Lark, Feishu), NyxID receives messages via platform webhooks, normalizes them into a common format, routes each message to the correct AI agent's callback URL, and relays the agent's response back to the chat.

Combined with [Agent Isolation](./AGENT_ISOLATION.md), the same NyxID user can wire different messaging platforms (or even different conversations on the same platform) to different AI agents -- each with independent credentials, rate limits, and audit trails.

---

## Problem Statement

Today, connecting an AI agent to a messaging platform requires:

1. **Per-platform bot infrastructure** -- each agent team builds and hosts their own Telegram/Discord/Lark bot
2. **Platform-specific code** -- webhook verification, message parsing, reply formatting differs per platform
3. **No centralized credential management** -- bot tokens scattered across agent configs
4. **No unified audit trail** -- no visibility into which agent handled which message
5. **No agent routing** -- can't send Telegram DMs to Claude and Discord messages to GPT without separate bots

NyxID already solves the equivalent problem for API credentials (proxy gateway). Channel Bot Relay extends this to messaging.

---

## High-Level Architecture

```mermaid
graph TB
    subgraph Messaging Platforms
        TG[Telegram]
        DC[Discord]
        LK[Lark]
        FS[Feishu]
    end

    subgraph NyxID
        WH[Webhook Handlers<br/>per-platform endpoints]
        PA[Platform Adapters<br/>normalize + verify]
        RS[Routing Service<br/>conversation -> agent]
        RL[Relay Service<br/>callback + reply]
        DB[(MongoDB<br/>channel_bots<br/>channel_conversations<br/>channel_messages)]
    end

    subgraph AI Agents
        A1[Claude Code<br/>callback URL A]
        A2[GPT Agent<br/>callback URL B]
        A3[Custom Agent<br/>callback URL C]
    end

    TG -->|webhook| WH
    DC -->|webhook| WH
    LK -->|webhook| WH
    FS -->|webhook| WH

    WH --> PA
    PA --> RS
    RS --> RL
    RL -->|lookup/store| DB

    RL -->|POST normalized msg| A1
    RL -->|POST normalized msg| A2
    RL -->|POST normalized msg| A3

    A1 -->|reply body| RL
    A2 -->|reply body| RL
    A3 -->|reply body| RL

    RL -->|send_reply| PA
    PA -->|platform API| TG
    PA -->|platform API| DC
    PA -->|platform API| LK
    PA -->|platform API| FS
```

---

## Message Flow

### Inbound: Platform -> Agent

```mermaid
sequenceDiagram
    participant U as User (Telegram/Discord/etc.)
    participant P as Platform API
    participant W as NyxID Webhook Handler
    participant A as Platform Adapter
    participant R as Routing Service
    participant RL as Relay Service
    participant DB as MongoDB
    participant AG as AI Agent (callback URL)

    U->>P: Send message
    P->>W: POST /api/v1/webhooks/channel/{platform}
    W->>A: verify_webhook(headers, body)
    A-->>W: OK (signature valid)
    W->>A: parse_inbound(body)
    A-->>W: Vec<InboundMessage>

    loop For each InboundMessage
        W->>R: resolve_agent(bot_id, conversation_id, sender_id)
        R->>DB: Lookup channel_conversations
        DB-->>R: (agent_api_key_id, callback_url)
        R-->>W: AgentRoute

        W->>DB: Insert channel_message (direction: inbound)

        W->>RL: forward_to_agent(message, callback_url)
        RL->>AG: POST callback_url<br/>X-NyxID-Signature: HMAC<br/>X-NyxID-Message-Id: uuid

        alt Sync Reply (200 + body)
            AG-->>RL: { reply: { text: "..." } }
            RL->>A: send_reply(bot, conversation_id, reply)
            A->>P: Platform send message API
            P->>U: Display reply
            RL->>DB: Insert channel_message (direction: outbound)
        else Async Ack (202)
            AG-->>RL: 202 Accepted
            Note over RL: Agent will call /channel-relay/reply later
        else Error/Timeout
            RL->>DB: Update callback_status = "failed"
        end
    end

    W-->>P: 200 OK (always, to prevent platform retries)
```

### Async Reply: Agent -> Platform

```mermaid
sequenceDiagram
    participant AG as AI Agent
    participant H as NyxID Reply Handler
    participant DB as MongoDB
    participant A as Platform Adapter
    participant P as Platform API
    participant U as User

    AG->>H: POST /api/v1/channel-relay/reply<br/>Authorization: Bearer {api_key}
    H->>DB: Lookup channel_message by message_id
    H->>DB: Verify api_key_id matches conversation's agent
    H->>DB: Lookup channel_bot (get encrypted token)
    H->>A: send_reply(bot, conversation_id, reply)
    A->>P: Platform send message API
    P->>U: Display reply
    H->>DB: Insert channel_message (direction: outbound)
    H-->>AG: 200 OK { platform_message_id: "..." }
```

### Bot Registration

```mermaid
sequenceDiagram
    participant U as User (authenticated)
    participant H as NyxID Bot Handler
    participant A as Platform Adapter
    participant P as Platform API
    participant DB as MongoDB

    U->>H: POST /api/v1/channel-bots<br/>{ platform: "telegram", bot_token: "123:ABC" }
    H->>A: verify_bot_token(bot_token)
    A->>P: GET /getMe (or equivalent)
    P-->>A: { id: "bot123", username: "MyBot" }
    A-->>H: BotIdentity

    H->>DB: Check max_bots_per_user limit
    H->>H: Encrypt bot_token (AES-256)
    H->>H: Generate webhook_secret (32 bytes)
    H->>DB: Insert channel_bot (status: pending_verification)

    H->>A: register_webhook(bot, webhook_url, secret)
    A->>P: POST /setWebhook (or equivalent)
    P-->>A: OK

    H->>DB: Update channel_bot (status: active, webhook_registered: true)
    H-->>U: 201 Created { id, platform, bot_username, status }
```

---

## Agent Routing & Isolation

### How Conversations Map to Agents

```mermaid
graph TD
    MSG[Inbound Message] --> R{Routing Service}

    R -->|Step 1| EC{Exact conversation<br/>match?}
    EC -->|Yes| AGENT[Route to bound agent]
    EC -->|No| SS{Step 2: Sender-specific<br/>match in group?}
    SS -->|Yes| AGENT
    SS -->|No| DA{Step 3: Default agent<br/>for this bot?}
    DA -->|Yes| AGENT
    DA -->|No| UNROUTED[Log as unrouted<br/>Optional: send 'not configured' reply]

    AGENT --> CB[POST agent callback_url]
```

### Integration with Agent Isolation (PR #132)

The callback URL lives on the **ApiKey** (the agent), not on individual conversation routes. When a user sets up an agent on NyxID (`nyxid ai-setup agent create --platform claude-code`), they register the agent's callback URL as part of the agent configuration. Conversation routes then just say "send to this agent" -- NyxID already knows how to reach it.

This means:
- **`ApiKey.callback_url`** (new field) -- where NyxID sends channel messages for this agent
- **`ChannelConversation.agent_api_key_id`** -- which agent handles this conversation (callback URL resolved from the API key)
- No `agent_callback_url` on the conversation route -- the URL is a property of the agent, not the conversation

```mermaid
graph LR
    subgraph Channel Relay Layer
        BOT[Channel Bot<br/>Telegram / Discord / Lark]
        CONV[Channel Conversation<br/>agent_api_key_id]
    end

    subgraph Agent Isolation Layer
        AK[ApiKey<br/>platform, callback_url<br/>rate limits, scopes]
        ASB[AgentServiceBinding<br/>per-agent credential override]
    end

    subgraph Proxy Layer
        PS[Proxy Service<br/>credential injection<br/>scope enforcement]
    end

    CONV -->|references| AK
    AK -->|scopes| ASB
    ASB -->|overrides credentials at| PS

    BOT -->|receives messages for| CONV
    CONV -->|forwards to agent via| AK
```

The relay and proxy are **parallel paths, not nested**:

- **Relay path**: Platform -> NyxID webhook -> agent callback URL (message forwarding)
- **Proxy path**: Agent -> NyxID proxy -> downstream API (credential injection)

An agent receiving a message via the relay can then call external APIs through NyxID's proxy using its scoped API key. The agent isolation scope enforcement applies to the proxy call, not the relay.

---

## Data Model

### Entity Relationship

```mermaid
erDiagram
    User ||--o{ ChannelBot : registers
    User ||--o{ ApiKey : owns
    ChannelBot ||--o{ ChannelConversation : has
    ApiKey ||--o{ ChannelConversation : "routes to"
    ApiKey ||--o{ AgentServiceBinding : "binds credentials via"
    ChannelConversation ||--o{ ChannelMessage : contains
    ChannelBot ||--o{ ChannelMessage : "sent/received via"

    ApiKey {
        string id PK "existing model -- new field added"
        string name "human-readable agent name"
        string platform "claude-code | codex | openclaw | generic"
        string callback_url "NEW: where to POST channel messages"
        int rate_limit_per_second "per-agent rate limit"
        int rate_limit_burst "per-agent burst"
        array allowed_service_ids "proxy scope"
        array allowed_node_ids "proxy scope"
    }

    ChannelBot {
        string id PK
        string user_id FK
        string platform "telegram | discord | lark | feishu"
        string label
        bytes bot_token_encrypted
        string platform_bot_id
        string platform_bot_username
        bool webhook_registered
        string webhook_secret_hash
        string status "pending | active | failed | invalid"
        string app_id "Lark/Feishu only"
        bytes app_secret_encrypted "Lark/Feishu only"
        string public_key "Discord only"
        bool is_active
        datetime created_at
        datetime updated_at
    }

    ChannelConversation {
        string id PK
        string user_id FK
        string channel_bot_id FK
        string platform
        string platform_conversation_id
        string platform_conversation_type "private | group | channel"
        string platform_sender_id "optional: per-sender routing in groups"
        string agent_api_key_id FK "which agent handles this"
        bool default_agent "fallback route for unmatched conversations"
        bool is_active
        datetime last_message_at
        datetime created_at
        datetime updated_at
    }

    ChannelMessage {
        string id PK
        string channel_bot_id FK
        string conversation_id FK
        string user_id FK
        string direction "inbound | outbound"
        string platform
        string platform_message_id
        string sender_platform_id
        string sender_display_name
        string content_type "text | image | file | audio | video"
        string text
        array attachments "MessageAttachment[]"
        object raw_platform_data "original JSON for debugging"
        string agent_api_key_id FK
        string callback_status "pending | delivered | failed | timeout"
        string reply_to_message_id FK "for outbound: which inbound this replies to"
        string platform_reply_message_id
        datetime created_at "TTL: 30 days"
    }
```

### MongoDB Indexes

| Collection | Index | Type | Purpose |
|---|---|---|---|
| `channel_bots` | `{ user_id: 1, platform: 1 }` | Unique | One bot per platform per user |
| `channel_bots` | `{ platform: 1, platform_bot_id: 1 }` | Standard | Webhook bot lookup |
| `channel_conversations` | `{ channel_bot_id: 1, platform_conversation_id: 1 }` | Unique | One mapping per conversation |
| `channel_conversations` | `{ user_id: 1, platform: 1 }` | Standard | List user's routes |
| `channel_conversations` | `{ agent_api_key_id: 1 }` | Standard | Find routes for an agent |
| `channel_messages` | `{ conversation_id: 1, created_at: -1 }` | Standard | Conversation history |
| `channel_messages` | `{ created_at: 1 }` | TTL (30d) | Auto-cleanup |

---

## Platform Adapter Trait

```mermaid
classDiagram
    class PlatformAdapter {
        <<trait>>
        +platform_id() str
        +verify_webhook(bot, headers, body) Result
        +parse_inbound(body) Result~Vec~InboundMessage~~
        +send_reply(http, bot, conversation_id, reply) Result~String~
        +register_webhook(http, bot, url, secret) Result
        +verify_bot_token(http, token) Result~BotIdentity~
        +handle_challenge(body) Option~JSON~
    }

    class TelegramAdapter {
        +platform_id() "telegram"
        -Secret header verification
        -Reuses telegram_service.rs
        -No challenge needed
    }

    class DiscordAdapter {
        +platform_id() "discord"
        -Ed25519 signature verification
        -PING/PONG challenge
        -Interaction-based model
    }

    class LarkFamilyAdapter {
        -base_url: String
        +platform_id() "lark" or "feishu"
        -HMAC-SHA256 verification
        -url_verification challenge
        -App access token caching
    }

    PlatformAdapter <|.. TelegramAdapter
    PlatformAdapter <|.. DiscordAdapter
    PlatformAdapter <|.. LarkFamilyAdapter

    note for LarkFamilyAdapter "Single implementation,\nregistered twice:\nlark = larksuite.com\nfeishu = feishu.cn"
```

### Platform-Specific Notes

| Platform | Auth Model | Webhook Verification | Challenge | Send Reply API |
|---|---|---|---|---|
| **Telegram** | Bot token (`123:ABC...`) | `X-Telegram-Bot-Api-Secret-Token` header (constant-time) | None | `POST /bot{token}/sendMessage` |
| **Discord** | Bot token + Application ID | Ed25519 signature (`X-Signature-Ed25519` + `X-Signature-Timestamp`) | `PING` -> `PONG` interaction response | `POST /channels/{id}/messages` with `Authorization: Bot {token}` |
| **Lark** | App ID + App Secret | HMAC-SHA256 on `X-Lark-Signature` header | `url_verification` event -> echo `challenge` | `POST /im/v1/messages` with tenant access token |
| **Feishu** | App ID + App Secret (same as Lark) | Same as Lark | Same as Lark | Same as Lark, different base URL (`open.feishu.cn`) |

---

## Callback Contract

### NyxID -> Agent (Webhook POST)

NyxID sends a normalized message to the agent's callback URL.

```json
{
  "message_id": "550e8400-e29b-41d4-a716-446655440000",
  "platform": "telegram",
  "agent": {
    "api_key_id": "880e8400-e29b-41d4-a716-446655440000",
    "name": "claude-support-bot"
  },
  "conversation": {
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "platform_id": "12345678",
    "type": "private"
  },
  "sender": {
    "platform_id": "87654321",
    "display_name": "John Doe"
  },
  "content": {
    "type": "text",
    "text": "What is the weather in Tokyo?",
    "attachments": []
  },
  "reply_to_message_id": null,
  "thread_id": null,
  "timestamp": "2026-03-31T12:00:00Z"
}
```

**Design rationale:**

The payload is intentionally lean -- transport identifiers only, no identity resolution on the hot path.

- **`agent.api_key_id`** is the primary agent identifier. Same `ApiKey._id` from agent isolation (PR #132). A shared callback endpoint dispatches based on this value. Use for routing/authorization.
- **`agent.name`** is the human-readable label from key creation (e.g., `"claude-support-bot"`). For logging and display only -- never use for authorization.
- **`sender.platform_id`** is the platform-native user ID. The agent is responsible for mapping this to its own users. If the agent uses NyxID OAuth, it already has `nyxid_user_id` in its user table from login time -- it can match `sender.platform_id` against platform identities it collected during onboarding. NyxID doesn't need to do this lookup because the agent already has the data.
- **No NyxID user IDs for senders** -- the agent knows the bot owner (from its API key), and knows the sender (from its own user table or the optional resolve-sender API). NyxID's job is message transport, not identity resolution.
- **No PII** -- no emails, no NyxID-stored names. `sender.display_name` is platform-provided (Telegram `first_name`, Discord `username`).

**Field Reference:**

| Field | Type | Nullable | Description |
|---|---|---|---|
| `message_id` | UUID | No | NyxID's internal ID for this message record (stored in `channel_messages`). The agent uses this to send async replies via `POST /channel-relay/reply`. |
| `platform` | string | No | Which messaging platform the message came from: `telegram`, `discord`, `lark`, or `feishu`. |
| `agent.api_key_id` | UUID | No | The `ApiKey._id` assigned to this conversation route. This is the agent's identity from agent isolation. A shared callback endpoint dispatches based on this. |
| `agent.name` | string | No | Human-readable name of the API key (e.g., `"claude-support-bot"`). For logging and display only. |
| `conversation.id` | UUID | No | NyxID's internal ID for the conversation route (from `channel_conversations`). Stable across all messages in the same chat. |
| `conversation.platform_id` | string | No | The platform's native conversation identifier (Telegram `chat_id`, Discord `channel_id`, Lark `chat_id`). |
| `conversation.type` | string | No | Conversation kind: `private` (1:1 DM), `group` (multi-user chat), or `channel` (broadcast). |
| `sender.platform_id` | string | No | The message author's ID on the platform. The agent maps this to its own users. |
| `sender.display_name` | string | Yes | Display name from the platform (Telegram `first_name`, Discord `username`). `null` if not provided. |
| `content.type` | string | No | Content kind: `text`, `image`, `file`, `audio`, `video`, `location`, `sticker`, or `unknown`. |
| `content.text` | string | Yes | Text body. Present for `text`; may contain caption for media. `null` for non-text without caption. |
| `content.attachments` | array | No | Non-text attachments: `{ type, url, filename, mime_type, size_bytes }`. Empty `[]` for plain text. |
| `reply_to_message_id` | UUID | Yes | NyxID `message_id` of the message being replied to. `null` for standalone messages. |
| `thread_id` | string | Yes | Platform-native thread ID (Discord threads, Lark threads). `null` if not in a thread. |
| `timestamp` | ISO 8601 | No | When the message was sent on the platform (not when NyxID received it). |

**Headers:**

| Header | Description |
|---|---|
| `Content-Type` | `application/json` |
| `X-NyxID-Signature` | HMAC-SHA256 of request body, signed with the API key's hash |
| `X-NyxID-Message-Id` | UUID of the `channel_message` record |
| `X-NyxID-Timestamp` | ISO 8601 timestamp (for replay protection) |
| `X-NyxID-Platform` | Platform identifier (`telegram`, `discord`, `lark`, `feishu`) |

### Identity Resolution (optional convenience API)

For agents that don't maintain their own user-to-platform mapping, NyxID provides a lookup endpoint. This is a convenience -- most agents integrated with NyxID OAuth already have this data from user onboarding.

```
GET /api/v1/channel-relay/resolve-sender?platform=telegram&platform_id=87654321
Authorization: Bearer nyxid_ag_xxxxx
```

**Response (linked):**
```json
{
  "platform": "telegram",
  "platform_id": "87654321",
  "nyxid_user_id": "770e8400-e29b-41d4-a716-446655440000",
  "linked": true
}
```

**Response (not linked):**
```json
{
  "platform": "telegram",
  "platform_id": "87654321",
  "nyxid_user_id": null,
  "linked": false
}
```

**Resolution checks** (in order):
1. `notification_channels` -- Telegram `telegram_chat_id` matched against `platform_id`
2. `user_provider_tokens` -- Telegram identity tokens with `telegram_user_id` metadata
3. Future: dedicated `channel_identity_links` collection for explicit cross-platform mapping

Scoped to the bot owner's account -- only resolves identities linked to the user who registered the bot.

### Agent -> NyxID (Sync Reply, HTTP 200)

Agent returns a reply in the callback response body:

```json
{
  "reply": {
    "text": "The weather in Tokyo is 22C and sunny.",
    "reply_to_platform_message_id": "optional, for threading",
    "metadata": null
  }
}
```

**Reply Field Reference:**

| Field | Type | Nullable | Description |
|---|---|---|---|
| `reply.text` | string | Yes | The text response to send back to the chat. Required for text replies. |
| `reply.reply_to_platform_message_id` | string | Yes | Platform-native message ID to reply to (for threading). If set, the reply will appear as a threaded response on platforms that support it (Telegram reply, Discord thread, Lark thread). |
| `reply.metadata` | object | Yes | Platform-specific extras (e.g., Telegram `parse_mode`, Discord embed objects). Passed through to the platform adapter. `null` for plain text replies. |

### Agent -> NyxID (Async, HTTP 202 then POST later)

If the agent needs more time (LLM inference, tool calls, etc.), it returns `202 Accepted` with an empty body, then calls back when ready:

```
POST /api/v1/channel-relay/reply
Authorization: Bearer nyxid_ag_xxxxx
Content-Type: application/json

{
  "message_id": "550e8400-e29b-41d4-a716-446655440000",
  "reply": {
    "text": "After checking multiple sources, the weather in Tokyo is 22C and sunny with 60% humidity.",
    "metadata": null
  }
}
```

**Async Reply Field Reference:**

| Field | Type | Nullable | Description |
|---|---|---|---|
| `message_id` | UUID | No | The `message_id` from the original inbound callback payload. Identifies which message this reply is for, so NyxID can resolve the correct conversation and platform to send the reply to. |
| `reply.text` | string | Yes | The text response to send back to the chat. |
| `reply.metadata` | object | Yes | Platform-specific extras, same as sync reply. |

### Callback Flow Decision

```mermaid
flowchart TD
    CB[Agent Callback POST] --> STATUS{Response Status?}

    STATUS -->|200 + body| SYNC[Parse reply JSON]
    SYNC --> SEND[send_reply via adapter]
    SEND --> LOG_OUT[Log outbound message]

    STATUS -->|202 no body| ASYNC[Mark callback_status = delivered]
    ASYNC --> WAIT[Agent calls /channel-relay/reply later]
    WAIT --> SEND

    STATUS -->|4xx / 5xx| ERR[Mark callback_status = failed]
    ERR --> OPT{Send error<br/>msg to chat?}
    OPT -->|configurable| ERRMSG[Platform: 'Agent unavailable']
    OPT -->|no| DONE[Done]

    STATUS -->|timeout| TO[Mark callback_status = timeout]
    TO --> OPT
```

---

## API Endpoints

### Bot Management (authenticated, human-only)

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/v1/channel-bots` | Register a new bot |
| `GET` | `/api/v1/channel-bots` | List user's bots |
| `GET` | `/api/v1/channel-bots/{id}` | Get bot details |
| `DELETE` | `/api/v1/channel-bots/{id}` | Delete bot (deregisters webhook) |
| `POST` | `/api/v1/channel-bots/{id}/verify` | Re-verify bot token and webhook |

### Conversation Routes (authenticated, human-only)

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/v1/channel-conversations` | Create conversation -> agent route (callback URL resolved from `ApiKey.callback_url`) |
| `GET` | `/api/v1/channel-conversations` | List user's routes (filterable by bot, platform, agent) |
| `GET` | `/api/v1/channel-conversations/{id}` | Get route details |
| `PUT` | `/api/v1/channel-conversations/{id}` | Update route (change agent) |
| `DELETE` | `/api/v1/channel-conversations/{id}` | Delete route |

### Relay (API-key authenticated)

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/v1/channel-relay/reply` | Agent sends async reply to a message |
| `GET` | `/api/v1/channel-relay/messages/{conversation_id}` | Get conversation message history |
| `GET` | `/api/v1/channel-relay/resolve-sender` | Resolve a platform sender to a NyxID user (query params: `platform`, `platform_id`) |

### Platform Webhooks (unauthenticated, signature-verified)

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/v1/webhooks/channel/telegram` | Telegram bot webhook |
| `POST` | `/api/v1/webhooks/channel/discord` | Discord interaction webhook |
| `POST` | `/api/v1/webhooks/channel/lark` | Lark event webhook |
| `POST` | `/api/v1/webhooks/channel/feishu` | Feishu event webhook |

---

## Security

### Threat Model

```mermaid
graph TD
    subgraph Threats
        T1[SSRF via callback URL]
        T2[Bot token leakage]
        T3[Webhook forgery]
        T4[Replay attacks]
        T5[Message injection in group chats]
        T6[Agent impersonation on async reply]
    end

    subgraph Mitigations
        M1[Block private IPs, HTTPS-only in prod]
        M2[AES-256 at rest, never in API responses]
        M3[Per-platform signature verification]
        M4[X-NyxID-Timestamp + replay window]
        M5[platform_sender_id scoping on routes]
        M6[api_key_id must match conversation agent]
    end

    T1 --> M1
    T2 --> M2
    T3 --> M3
    T4 --> M4
    T5 --> M5
    T6 --> M6
```

| Concern | Mitigation |
|---|---|
| **SSRF** | Callback URLs validated: HTTPS-only in production, block RFC 1918/loopback ranges, optional domain allowlist |
| **Bot token storage** | AES-256 encrypted at rest (same pattern as `UserApiKey.credential_encrypted`). Never returned in API responses. Only `platform_bot_username` is exposed. |
| **Webhook forgery** | Per-platform verification: Telegram secret header, Discord Ed25519, Lark HMAC-SHA256. All constant-time comparison. |
| **Replay attacks** | Callbacks include `X-NyxID-Timestamp`. Agents should reject messages older than 5 minutes. |
| **Callback authentication** | `X-NyxID-Signature` is HMAC-SHA256 of the request body, keyed with the API key's hash. Agents verify this to confirm the request came from NyxID. |
| **Agent impersonation** | Async reply endpoint requires the calling API key to match the conversation's `agent_api_key_id`. |
| **Rate limiting** | Per-bot rate limiting on inbound webhooks. Per-agent rate limiting on callback dispatch (reuses `PerAgentRateLimiter` from agent isolation). |

---

## Implementation Phases

### Phase 1: Foundation

Models, platform adapter trait, error codes, config.

```mermaid
gantt
    title Phase 1 - Foundation
    dateFormat  X
    axisFormat %s

    section Models
    channel_bot.rs           :a1, 0, 1
    channel_conversation.rs  :a2, 0, 1
    channel_message.rs       :a3, 0, 1
    Register in mod.rs       :a4, after a1, 1

    section Infrastructure
    Error variants (10000-10005)  :b1, 0, 1
    Config env vars               :b2, 0, 1
    DB indexes                    :b3, after a1, 1

    section Trait
    PlatformAdapter trait     :c1, 0, 1
    Normalized types          :c2, 0, 1
```

**New files:**
- `backend/src/models/channel_bot.rs`
- `backend/src/models/channel_conversation.rs`
- `backend/src/models/channel_message.rs`
- `backend/src/services/channel_platform.rs` (trait + types)

**Modified files:**
- `backend/src/models/mod.rs` -- register modules
- `backend/src/services/mod.rs` -- register module
- `backend/src/errors/mod.rs` -- new error variants
- `backend/src/config.rs` -- new env vars
- `backend/src/db.rs` -- new indexes

### Phase 2: Telegram Adapter

First platform adapter, reuses existing `telegram_service.rs`.

**New files:**
- `backend/src/services/channel_adapters/mod.rs`
- `backend/src/services/channel_adapters/telegram.rs`

### Phase 3: Core Services

Bot CRUD, conversation routing, relay orchestration.

**New files:**
- `backend/src/services/channel_bot_service.rs`
- `backend/src/services/channel_routing_service.rs`
- `backend/src/services/channel_relay_service.rs`

### Phase 4: HTTP Handlers & Routes

Wire up all endpoints.

**New files:**
- `backend/src/handlers/channel_bots.rs`
- `backend/src/handlers/channel_webhooks.rs`
- `backend/src/handlers/channel_relay.rs`

**Modified files:**
- `backend/src/handlers/mod.rs`
- `backend/src/routes.rs`
- `backend/src/main.rs` (webhook health check background task)

### Phase 5: Discord, Lark, Feishu Adapters

Remaining platform adapters.

**New files:**
- `backend/src/services/channel_adapters/discord.rs`
- `backend/src/services/channel_adapters/lark.rs`

**New dependencies:**
- `ed25519-dalek` (Discord signature verification)

### Phase 6: OpenClaw Bridge Migration

Migrate existing `openclaw_channel_mappings` to the generic relay. Backward-compatible dual-path lookup.

**New files:**
- `backend/src/services/channel_adapters/openclaw.rs`

**Modified files:**
- `backend/src/handlers/openclaw_channel.rs` (dual-path lookup)

### Phase 7: Frontend

Bot management UI, conversation route editor, message log.

**New files:**
- `frontend/src/types/channels.ts`
- `frontend/src/hooks/use-channels.ts`
- `frontend/src/schemas/channels.ts`
- `frontend/src/pages/channel-bots.tsx`
- `frontend/src/pages/channel-bot-detail.tsx`
- `frontend/src/components/dashboard/add-channel-bot-dialog.tsx`
- `frontend/src/components/dashboard/channel-route-editor.tsx`

**Modified files:**
- `frontend/src/router.tsx`
- `frontend/src/components/dashboard/sidebar.tsx`

---

## Phase Dependency Graph

```mermaid
graph LR
    P1[Phase 1<br/>Foundation] --> P2[Phase 2<br/>Telegram Adapter]
    P1 --> P3[Phase 3<br/>Core Services]
    P2 --> P3
    P3 --> P4[Phase 4<br/>Handlers & Routes]
    P4 --> P5[Phase 5<br/>Discord / Lark / Feishu]
    P4 --> P6[Phase 6<br/>OpenClaw Migration]
    P4 --> P7[Phase 7<br/>Frontend]

    style P1 fill:#e1f5fe
    style P2 fill:#e1f5fe
    style P3 fill:#fff3e0
    style P4 fill:#fff3e0
    style P5 fill:#f3e5f5
    style P6 fill:#f3e5f5
    style P7 fill:#e8f5e9
```

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `CHANNEL_RELAY_CALLBACK_TIMEOUT_SECS` | `30` | HTTP timeout for agent callback requests |
| `CHANNEL_RELAY_MAX_BOTS_PER_USER` | `5` | Maximum bots a user can register |
| `CHANNEL_RELAY_MESSAGE_TTL_DAYS` | `30` | TTL for `channel_messages` auto-cleanup |

---

## Relationship to Existing Systems

| Existing System | Relationship | Migration Path |
|---|---|---|
| **Telegram approval bot** (system-level) | Completely separate. The admin's global bot for approval notifications is untouched. | None needed |
| **Telegram Login Widget** (identity provider) | Separate. Uses Telegram for authentication, not messaging. | None needed |
| **OpenClaw channel bridge** | Superseded. The new relay is a generalized version. | Phase 6: dual-path lookup, gradual migration |
| **Agent isolation** (PR #132) | Complementary. `ApiKey.id` is the `agent_api_key_id` reference. Proxy scope enforcement applies when agents make proxy calls. | Already integrated via shared `ApiKey` model |
| **Proxy gateway** | Parallel path. Relay forwards messages; proxy forwards API calls. Agents may use both. | None needed |

---

## Example: End-to-End Scenario

### Setup (one-time)

```mermaid
sequenceDiagram
    participant U as User
    participant N as NyxID
    participant TG as Telegram API

    Note over U,N: Step 1: Register agent with callback URL
    U->>N: nyxid ai-setup agent create<br/>--name claude-support<br/>--platform claude-code<br/>--callback-url https://my-claude.example.com/webhook
    N-->>U: API key: nyxid_ag_xxxxx (api_key_id: 880e...)

    Note over U,N: Step 2: Register Telegram bot
    U->>N: POST /api/v1/channel-bots<br/>{ platform: "telegram", bot_token: "123:ABC" }
    N->>TG: getMe (verify token)
    TG-->>N: { username: "MySupportBot" }
    N->>TG: setWebhook (register NyxID webhook URL)
    N-->>U: Bot registered (id: 660e...)

    Note over U,N: Step 3: Route conversations to agent
    U->>N: POST /api/v1/channel-conversations<br/>{ channel_bot_id: "660e...",<br/>  agent_api_key_id: "880e...",<br/>  default_agent: true,<br/>  resolve_sender_identity: true }
    N-->>U: Route created -- all DMs to MySupportBot go to claude-support
```

The callback URL is on the **agent** (API key), not the conversation route. If the user later creates a second agent ("gpt-research") with a different callback URL and routes a Discord bot to it, the same pattern applies.

### Runtime

```mermaid
sequenceDiagram
    participant Alice as Alice (Telegram)
    participant TG as Telegram API
    participant N as NyxID
    participant C as Claude Agent

    Alice->>TG: "Summarize my emails"
    TG->>N: Webhook POST (message from Alice in DM)
    N->>N: Verify Telegram signature
    N->>N: Parse message, resolve route -> claude-support (api_key_id: 880e...)
    N->>N: Resolve callback_url from ApiKey: https://my-claude.example.com/webhook
    N->>C: POST https://my-claude.example.com/webhook<br/>{ agent: { api_key_id: "880e...", name: "claude-support" },<br/>  sender: { platform_id: "87654321", nyxid_user_id: "770e..." },<br/>  content: { text: "Summarize my emails" } }
    C->>C: Match nyxid_user_id to internal user (from NyxID OAuth)
    C->>C: Process with LLM + tools
    C-->>N: 200 { reply: { text: "You have 3 unread..." } }
    N->>TG: sendMessage(chat_id, "You have 3 unread...")
    TG->>Alice: "You have 3 unread..."
```

**Meanwhile, on Discord (same user, different agent):**

```mermaid
sequenceDiagram
    participant Bob as Bob (Discord)
    participant DC as Discord API
    participant N as NyxID
    participant G as GPT Agent

    Bob->>DC: "Generate a report"
    DC->>N: Webhook POST (interaction from Bob)
    N->>N: Verify Ed25519 signature
    N->>N: Parse message, resolve route -> gpt-research (api_key_id: 990e...)
    N->>N: Resolve callback_url from ApiKey: https://my-gpt.example.com/webhook
    N->>G: POST https://my-gpt.example.com/webhook<br/>{ agent: { api_key_id: "990e...", name: "gpt-research" },<br/>  content: { text: "Generate a report" } }
    G-->>N: 202 Accepted (async, needs time)
    N-->>DC: Ack interaction

    Note over G: Agent processes for 30 seconds...

    G->>N: POST /channel-relay/reply<br/>Authorization: Bearer nyxid_ag_yyyyy<br/>{ message_id: "...", reply: { text: "Report: ..." } }
    N->>DC: Create message in channel
    DC->>Bob: "Report: ..."
```
