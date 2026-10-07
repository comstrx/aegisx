# Backend and control contracts · v1 / server 0.5

## Identity and correlation

`set_identity` controls request/forwarding header names, propagation, trusted UUID preservation, actor header, trusted peers and backend block signal. Generated request IDs are forwarded and echoed; they are correlation, never authentication or idempotency. Carry them into backend operations and queue-job metadata. A concurrently duplicated trusted ID is replaced when the active lifecycle registry detects it; sequential reuse remains possible, so producers must keep IDs unique.

Actor handles are HMAC-SHA256 hex. With storage enabled the random key lives in owner-only SQLite and survives restart; without storage it is ephemeral. Do not delete this key while expecting stable restrictions. Direct peer IP is retained in full received events. There is no trusted multi-hop forwarding resolver; NAT clients share an IP scope.

Only explicitly trusted direct peers may supply an actor header. The upstream authenticating gateway must overwrite client-supplied identity and supply a unique tenant/account composite. AegisX does not authenticate arbitrary user tokens. Header names cannot override transport/framing/credential headers.

## Decision repository

`set_cache {decisions=true}` requires storage. Decisions are keyed by actor plus a stable route/policy fingerprint, have a fixed expiry and apply before forwarding. Reads hit the local cache or fall back to bounded SQLite lookup. Positive and negative cache values are published by the serialized repository worker; a successful write response follows database commit and cache update.

`POST /blocks` accepts `config_version,route,actor,ttl_ms,reason` and optional UUID `request_id`. Use actual event values. Actor is 64 hex characters; duration cannot exceed configured deny TTL. Stale configuration or disabled route enforcement returns 409; unknown route 404; malformed data 400. `POST /blocks/revoke {"key":"…"}` deletes the durable decision and updates the cache. Operator/backend changes invalidate the authority of previously queued model jobs through a revision guard. The same guard is rechecked inside the writer when a model requests cooperative cancellation.

`POST /cache/purge {"kind":"all"}` accepts all/decisions/scores/responses. It **does not revoke bans**; a later miss loads them from SQLite. Response-cache purge also isolates already-running fills. Cache TTL, record expiry and policy scope are separate. Direct SQL edits are unsupported. A timed-out queued write reports `outcome:"unknown"`; inspect the repository before assuming it failed.

Backend responses can use the configured `backend_block_header` with positive integer seconds. It is stripped, duration-clamped and submitted to the repository if decisions are enabled. The current response completes. Following requests can still race before the asynchronous commit; this is not an atomic backend transaction. Cached denials do not refresh their TTL or repeat risk webhooks.

## Backend operation reports

Enable a separate credential:

```lua
set_control {
    enabled=true, listen="127.0.0.1:9090",
    token_env="AEGISX_ADMIN_TOKEN", backend_token_env="AEGISX_BACKEND_TOKEN"
}
```

Tokens are different 32–4096-character printable ASCII secrets supplied via the service environment. Admin credentials cannot call backend endpoints; backend credentials cannot administer the proxy.

`POST /backend/events` accepts exactly:

```json
{"request_id":"UUID","service":"billing","operation":"commit","state":"completed","duration_ms":12}
```

Service/operation are 1–96 ASCII characters from letters/digits/`._:/-`; state is started/completed/failed; optional duration is 0–86,400,000 ms. Optional span_id/parent_id are 1–64 ASCII letters/digits/underscore/hyphen; self-parenting is invalid. Optional elapsed_ms is 0–86,400,000 from request start; omission is stamped at proxy receipt. The model resolves a parent to its first earlier event; missing parents remain explicitly unknown. Unknown fields are rejected. No arbitrary parameters, credentials, SQL text or stack dumps. Use public operation names, not user data.

Reports are accepted only for an actively tracked request, at most 32 per journey. Missing/finished requests or exhausted budget return 409; success is 202. Capture-disabled journeys may inform the pending model envelope but are not persisted by this endpoint. The combined proxy/backend event list is bounded to 64 entries; backend reports are additionally capped at 32. Completed journeys commit together after analysis. A 202 accepts an in-memory backend report, not a durability acknowledgement. The panel marks whether backend operations were reported; it cannot discover arbitrary backend code execution.

## Cooperative cancellation

Opt in per route; storage and backend integration are required:

```lua
add_route {name="jobs",path="/jobs",cancellation=true,cancellation_ttl_ms=60000}
```

The fixed safety boundary:
1. Before forwarding, current explicit rules/decisions can reject.
2. After forwarding begins, new AI/operator verdicts affect subsequent admissions.
3. A cancellation intent asks the backend to act safely; it never closes the original stream or promises rollback.

Admin `POST /cancellations {"request_id":"UUID","route":"jobs"}` returns 202 after persisting an intent. Duplicate request/route intents within the deadline reuse the action ID. Background-mode risky analysis can create an intent on an opted-in route; observe mode cannot. A signed `cancellation_requested` webhook is an optional notification. The durable backend polling endpoint works even when delivery fails.

Backend `GET /backend/cancellations` returns pending requested/accepted intents (bounded to 200 records; oldest pending first). Admin `GET /cancellations` shows current intents including reported terminal outcomes. Maximum 10,000 live records; expired intents cease to be actionable. Retention is the configured intent deadline, not an unlimited audit log.

Backend `POST /backend/cancellations/ack` body:

```json
{"action_id":"UUID","state":"cancelled"}
```

States: requested → accepted → cancelled/too_late/unsupported; direct requested → terminal is allowed. Repeated identical acknowledgements are idempotent. Terminal states cannot regress. **Accepted means receipt, not stopped work.** Expired/nonexistent actions cannot be changed. The receiver should persist/deduplicate the intent and check before enqueue, execution, retries and safe checkpoints. Only backend application logic can judge whether cancellation is safe. Acknowledgements report what that trusted receiver asserts.

## Signed webhooks

Endpoints use HTTPS (or explicit loopback HTTP), no redirects or environment proxy settings. Filters: blocked, risk_detected, backend_signal, cancellation_requested; empty means all. `secret_env` is a 32–4096-byte raw UTF-8 signing key, not a base64-decoded whsec key.

HMAC-SHA256 signs `webhook-id + "." + webhook-timestamp + "." + exact raw HTTP body`. Headers are `webhook-id`, Unix-seconds `webhook-timestamp`, `webhook-signature: v1,BASE64_DIGEST`, and JSON content type. Retries preserve event ID and refresh timestamp. Verify raw bytes in constant time, enforce timestamp tolerance and deduplicate event IDs. Schema: [v1.json](v1.json).

Network errors/429/5xx get bounded retries; other statuses, including redirects, are terminal. A 2xx acknowledges delivery only. Webhook delivery itself is a memory-only best-effort queue: crash/overflow can lose notifications. Cancellation intents persist separately and can be polled. There is no exactly-once delivery guarantee.

## Control surface

The configured prefix defaults to /api/v1. Loopback listener only; Bearer authentication, matching Host and same-origin checks. Bodies are capped at 4 KiB; responses use no-store. This is one local operator authority, not a multi-user RBAC system.

| Endpoint | Method | Credential / result |
| --- | --- | --- |
| /state | GET | Admin; live metrics, active journeys, policies, worker/cache/upstream/process state |
| /requests/UUID | GET | Admin; up to 200 retained lifecycle events |
| /contracts | GET | Admin; webhook JSON Schema |
| /decisions | GET | Admin; up to 200 unexpired durable decisions |
| /blocks, /blocks/revoke | POST | Admin; commit/revoke future restrictions |
| /cache/purge | POST | Admin; evict cache entries |
| /cancellations | GET/POST | Admin; inspect/request cooperative actions |
| /backend/events | POST | Backend; bounded active-operation metadata |
| /backend/cancellations | GET | Backend; pending durable intents |
| /backend/cancellations/ack | POST | Backend; actual reported outcome |

Public static UI/bootstrap carry no operational state or token. `panel=false` disables assets while retaining the authenticated API. All API/data/model routes run locally; neither telemetry nor training data is uploaded.

 
In 0.8, analyzed event details add component_scores {risk,content,journey} and model_inputs (schema, byte/event counts, truncation, response availability, raw_content_persisted=false). These are research signals; a missing journey input is displayed as unavailable. Response/body text and arbitrary middleware parameters are not exposed through these records. Backend event names must be static public operation names, never SQL, credentials or user input.
