# WebSocket Auth-Frame Injection

NyxID can inject a held downstream credential into a WebSocket frame after the HTTP upgrade. This is for protocols that do not authenticate the upgrade request itself and instead send a challenge frame that expects a credential-bearing response frame.

Rules live on `DownstreamService.ws_frame_injections` and `UserService.ws_frame_injections`. `direction` is the trigger direction: `downstream` means the rule matches frames sent by the downstream service toward the client, and NyxID sends the injected frame back to the downstream service.

## Home Assistant

Home Assistant sends an auth challenge immediately after `/api/websocket` upgrades:

```json
{"type":"auth_required","ha_version":"..."}
```

A Home Assistant rule:

```json
{
  "trigger": {
    "json_field_equals": {
      "path": "$.type",
      "value": "auth_required"
    }
  },
  "template": "{\"type\":\"auth\",\"access_token\":\"${credential}\"}",
  "frame_kind": "text",
  "consume_trigger": true,
  "direction": "downstream"
}
```

Expected on-wire behavior:

```text
Downstream -> NyxID: {"type":"auth_required","ha_version":"..."}
NyxID -> Downstream: {"type":"auth","access_token":"<held credential>"}
Downstream -> Client: {"type":"auth_ok"}
```

With `consume_trigger: true`, NyxID hides the challenge from the client. The client sees only the post-auth downstream frames.

## Limits And Security

Each service can define at most 4 WebSocket frame injection rules. Each template is limited to 4096 bytes and only supports `${credential}` interpolation.

Each WebSocket connection can fire at most 8 injected frames. This prevents repeated challenge frames from causing unbounded credential replay.

Credentials are never included in logs, errors, or audit payloads. Successful injection emits the metadata-only audit event `ws_frame_auth_injected` with the service id, trigger kind, frame index, and node routing metadata when applicable.
