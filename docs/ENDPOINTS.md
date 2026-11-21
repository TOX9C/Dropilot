# Network Allowlist & Platform Endpoints

All outbound HTTP and WebSocket traffic in DropPilot passes through a single Rust module (`src-tauri/src/net/client.rs`) and is validated against `src-tauri/src/net/allowlist.rs` (including redirect targets). Non-allowlisted hosts, direct IP literals, embedded URL credentials, and insecure schemes (`http://`, `ws://`) are rejected before any socket is opened.

All platform calls are additionally throttled per platform in `src-tauri/src/net/rate_limit.rs` (sustained `<= 1 req/s`, burst `<= 5`), with `+-10%` jitter and exponential full-jitter backoff (`5s` to `15m`, honoring `Retry-After`).

---

## Allowlisted Hosts

| Platform / Feature | Allowlisted Hosts | Purpose |
| --- | --- | --- |
| **Twitch** | `twitch.tv`, `www.twitch.tv`, `gql.twitch.tv`, `id.twitch.tv`, `passport.twitch.tv`, `usher.ttvnw.net`, `*.ttvnw.net`, `static-cdn.jtvnw.net`, `*.jtvnw.net` | Official login window, token validation, web GQL operations, lowest-quality HLS segment HEAD watch tick, box art & reward thumbnails |
| **Kick** | `kick.com`, `*.kick.com` (`web.kick.com`, `websockets.kick.com`, `files.kick.com`), `*.live-video.net` | Official login window, drops APIs, channel livestreams, viewer presence WebSocket, compatibility playback CDNs |
| **Login IdPs (Login Window Only)** | `accounts.google.com`, `appleid.apple.com`, `challenges.cloudflare.com`, `*.google.com`, `*.apple.com`, `*.amazon.com`, `*.gstatic.com`, `*.googleapis.com` | Only permitted inside the embedded login window when a user chooses OAuth social login or verification on Twitch/Kick |
| **Update Check (Off by Default)** | `api.github.com` (`/repos/droppilot/droppilot/releases/latest` exact path only) | Optional check triggered only if enabled or manually clicked; only opens the release webpage in the browser |

---

## Twitch Endpoints (`src-tauri/src/platforms/twitch/gql.rs`)

- **Client identity:** Uses Twitch's public web `Client-ID` (`kimne78kx3ncx6brgo4mv6wki5h1ko`) paired with the embedded WebView2's own `User-Agent`. Never spoofs mobile, TV, or console clients.
- **Auth:** `Authorization: OAuth <token>` (obtained from `auth-token` cookie in DropPilot's own WebView2 after user login at `https://www.twitch.tv`).

| Operation / Endpoint | Purpose | Cadence | Auth | Status & Notes |
| --- | --- | --- | --- | --- |
| `GET https://id.twitch.tv/oauth2/validate` | Validate session token & retrieve `user_id`/`login` | On startup / login | `OAuth` header | Public official Twitch OAuth2 endpoint |
| `POST https://gql.twitch.tv/gql` (`CoreActionsCurrentUser`) | Fetch display name & avatar | On login / validation | Web `Client-ID` + `OAuth` | Persisted query hash isolated in `gql.rs`; verify in DevTools if rotated |
| `POST https://gql.twitch.tv/gql` (`ViewerDropsDashboard`) | Fetch active & upcoming drop campaigns | Every 30m (min 10m) | Web `Client-ID` + `OAuth` | Persisted query hash isolated in `gql.rs` |
| `POST https://gql.twitch.tv/gql` (`DropCampaignDetails`) | Fetch campaign allowed-channels list & account link status | On campaign refresh | Web `Client-ID` + `OAuth` | Persisted query hash isolated in `gql.rs` |
| `POST https://gql.twitch.tv/gql` (`Inventory`) | Fetch per-drop watch minutes, claimable state, and `dropInstanceID` | Every 5m + near drop completion | Web `Client-ID` + `OAuth` | Persisted query hash isolated in `gql.rs` |
| `POST https://gql.twitch.tv/gql` (`DirectoryPage_Game`) | Find live drops-enabled streams for a game | Only when selecting/switching | Web `Client-ID` + `OAuth` | Filters by Drops tag ID `c2542d6d-cd10-4532-919b-3dcc9f54d5bf` |
| `POST https://gql.twitch.tv/gql` (`UseLive`) | Verify whether watched channel is still live & game | Every 2.5m | Web `Client-ID` + `OAuth` | Persisted query hash isolated in `gql.rs` |
| `POST https://gql.twitch.tv/gql` (`PlaybackAccessToken_Template`) | Obtain HLS stream playback token & signature | ~12m cache per channel | Web `Client-ID` + `OAuth` | Persisted query hash isolated in `gql.rs` |
| `GET https://usher.ttvnw.net/api/channel/hls/{login}.m3u8` + lowest variant + `HEAD` latest segment | Lightweight watch presence without downloading or decoding video | ~60s | Signed playback token query | Never downloads video chunks; falls back to opt-in muted WebView2 if toggled |
| `POST https://gql.twitch.tv/gql` (`DropsPage_ClaimDropRewards`) | Claim earned drop via `dropInstanceID` | Within ~60s of completion | Web `Client-ID` + `OAuth` | Integrity challenges map to `Blocked` ("Claim needs you") without bypassing |

### How to verify or update Twitch persisted query hashes

Because Twitch rotates GraphQL persisted query SHA-256 hashes periodically (`PersistedQueryNotFound` triggers `AppError::PlatformChanged`):
1. Open `https://www.twitch.tv/drops/campaigns` and `https://www.twitch.tv/drops/inventory` in Edge/Chrome DevTools (**Network** tab, filter `gql`).
2. Inspect the JSON request body for `operationName` and `extensions.persistedQuery.sha256Hash`.
3. Update the corresponding constant inside `src-tauri/src/platforms/twitch/gql.rs` (`mod ops`).

---

## Kick Endpoints (`src-tauri/src/platforms/kick/api.rs`)

- **Auth:** `session_token` cookie captured from DropPilot's own WebView2 after user login at `https://kick.com` (sent as `Bearer` / session cookie).
- **Anti-bot handling:** Direct HTTP requests that hit a Cloudflare challenge (`403` / `cf-mitigated`) fall back transparently to executing the same `fetch()` inside DropPilot's hidden `https://kick.com` WebView2 page (where Kick's own scripts run normally). If that hidden page is also blocked, DropPilot stops and returns `AppError::Blocked`.

| Endpoint | Purpose | Cadence | Auth | Status & Notes |
| --- | --- | --- | --- | --- |
| `GET https://kick.com/api/v1/user` | Validate session & read username/avatar | On startup / login | Session token | Isolated in `kick/api.rs`; verify field names in DevTools if Kick changes schema |
| `GET https://web.kick.com/api/v1/drops/campaigns` | Fetch active/upcoming Kick drop campaigns | Every 30m (min 10m) | Session / cookies | Verify exact response keys (`starts_at`, `rewards`, `category`) against live JSON |
| `GET https://web.kick.com/api/v1/drops/progress` | Fetch per-reward watch progress & claimed flags | Every 5m + near drop completion | Session / cookies | Isolated in `kick/models.rs` (`parse_progress`) |
| `GET https://kick.com/stream/livestreams/en?category_id=...` | Discover live streams for campaign category | Only when selecting/switching | Session / cookies | Verify category query param against live Kick directory Network requests |
| `GET https://kick.com/api/v2/channels/{slug}` | Check live status, viewer count & current category | Every 2.5m | Session / cookies | Standard Kick v2 channel endpoint |
| `GET https://websockets.kick.com/viewer/v1/token` | Obtain short-lived viewer presence token | Per channel connect | Session / cookies | Returns `/data/token` |
| `WSS wss://websockets.kick.com/viewer/v1/connect?token=...` | Send `channel_handshake` and `{"type":"ping"}` viewer presence heartbeat | ~60s ping | `Origin: https://kick.com` + viewer token | Verify exact ping interval/payload in DevTools WS frames when testing live |
| `POST https://web.kick.com/api/v1/drops/claim` | Claim completed Kick drop reward | Within ~60s of completion | Session / cookies | Verify exact JSON payload (`reward_id`, `campaign_id`) against live claim request |

### How to verify Kick endpoints live

1. Open `https://kick.com/drops/campaigns` and a drops-enabled Kick stream in DevTools (**Network** tab, filter `Fetch/XHR` and `WS`).
2. Compare the request paths, JSON body keys, and WebSocket `type` frames against `src-tauri/src/platforms/kick/api.rs`, `models.rs`, and `watch.rs`.
3. Any HTTP `404` or unexpected JSON structure triggers `AppError::PlatformChanged` cleanly with `"Kick changed something; an update may be needed"`.
