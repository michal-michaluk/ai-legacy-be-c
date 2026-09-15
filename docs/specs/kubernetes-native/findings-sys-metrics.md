# Findings — current `$SYS` metric set (Q1)

Evidence for the Prometheus/OTel metrics capability. Paths are relative to
`mosquitto/`.

## Where it lives

| What | Location |
|---|---|
| Metric topic strings + aliases (master array) | `src/sys_tree.c:54-112` (`struct metric metrics[mosq_metric_max]`) |
| Load-rate topics (master array) | `src/sys_tree.c:114-142` (`struct metric_load metric_loads[…]`) |
| Metric enum (ordering contract) | `src/sys_tree.h:25-70` |
| Load enum | `src/sys_tree.h:73-105` |
| Periodic publish loop | `src/sys_tree.c:281-293` |
| `sys_tree__init()`, `sys_tree__update()` | `src/mosquitto.c:549-551`, `src/loop.c:205-208` |
| HTTP `/systree` (same values, JSON) | `src/http_api.c:242-260` |

## Emission mechanics

- All numeric metrics are published **retained**, **QoS 2** (`SYS_TREE_QOS`, `src/sys_tree.c:37`), infinite expiry.
- Payloads: `snprintf(buf,100,"%lu",…)` for metrics (`:286`), `"%.2f"` for load rates (`:161`), `"%llu seconds"` for uptime (`:243`).
- **Published only on change** (`next != current`; `is_max` metrics only on increase, `:282-283`).
- Refresh cadence: `sys_interval` seconds, default **10** (`src/conf.c:351`); fires when `db.now_real_s % sys_interval == 0` (`:239-240`).
- `sys_interval == 0` disables all `$SYS` publishing including `version` (`:175-177`).

## Static / scalar topics

| Topic | Value | Type | Unit | Gate | file:line |
|---|---|---|---|---|---|
| `$SYS/broker/version` | `mosquitto version <VER>` | static string | — | `WITH_SYS_TREE` + `sys_interval≠0` | `src/sys_tree.c:180-181` |
| `$SYS/broker/uptime` | seconds since start | monotonic counter | seconds | `WITH_SYS_TREE` + `sys_interval≠0` | `src/sys_tree.c:243-244` |

`$SYS/broker/timestamp` and `$SYS/broker/changeset` are **no longer emitted**.

## Numeric metric topics (`metrics[]`)

Type: `mosq_gauge_*` / `mosq_counter_*` (code classification). "on-event" = mutated as events
happen; "sampled" = overwritten from DB state just before publishing.

| # | Topic | Alias | Value | Type | Unit | file:line |
|---|---|---|---|---|---|---|
| 0 | `$SYS/broker/clients/total` | — | registered clients | gauge/sampled | count | `sys_tree.c:55` |
| 1 | `$SYS/broker/clients/maximum` | — | peak simultaneous clients | counter (`is_max`) | count | `sys_tree.c:56` |
| 2 | `$SYS/broker/clients/disconnected` | `…/clients/inactive` | disconnected persistent clients | gauge/sampled | count | `sys_tree.c:57` |
| 3 | `$SYS/broker/clients/connected` | `…/clients/active` | connected clients | gauge/sampled | count | `sys_tree.c:58` |
| 4 | `$SYS/broker/clients/expired` | — | expired persistent clients | counter/on-event | count | `sys_tree.c:59` |
| 5 | `$SYS/broker/messages/stored` | `…/store/messages/count` | queued messages | gauge/sampled | count | `sys_tree.c:60` |
| 6 | `$SYS/broker/store/messages/bytes` | — | queued message bytes | gauge/sampled | bytes | `sys_tree.c:61` |
| 7 | `$SYS/broker/subscriptions/count` | — | subscription count | gauge/sampled | count | `sys_tree.c:62` |
| 8 | `$SYS/broker/shared_subscriptions/count` | — | shared subscription count | gauge/sampled | count | `sys_tree.c:63` |
| 9 | `$SYS/broker/retained messages/count` | — | retained messages | gauge/sampled | count | `sys_tree.c:64` |
| 10 | `$SYS/broker/heap/current` | — | current heap usage | gauge/sampled | bytes | `sys_tree.c:66` |
| 11 | `$SYS/broker/heap/maximum` | — | peak heap usage | counter (`is_max`) | bytes | `sys_tree.c:67` |
| 12 | `$SYS/broker/messages/received` | — | messages received | counter/on-event | count | `sys_tree.c:72` |
| 13 | `$SYS/broker/messages/sent` | — | messages sent | counter/on-event | count | `sys_tree.c:73` |
| 14 | `$SYS/broker/bytes/received` | — | bytes received | counter/on-event | bytes | `sys_tree.c:74` |
| 15 | `$SYS/broker/bytes/sent` | — | bytes sent | counter/on-event | bytes | `sys_tree.c:75` |
| 16 | `$SYS/broker/publish/bytes/received` | — | PUBLISH payload bytes recv | counter/on-event | bytes | `sys_tree.c:76` |
| 17 | `$SYS/broker/publish/bytes/sent` | — | PUBLISH payload bytes sent | counter/on-event | bytes | `sys_tree.c:77` |
| 18 | `$SYS/broker/packet/out/count` | — | in-flight out packets | gauge/on-event | count | `sys_tree.c:78` |
| 19 | `$SYS/broker/packet/out/bytes` | — | in-flight out packet bytes | gauge/on-event | bytes | `sys_tree.c:79` |
| 20 | `$SYS/broker/connections/socket/count` | — | total sockets opened | counter/on-event | count | `sys_tree.c:80` |
| 25 | `$SYS/broker/publish/messages/dropped` | — | dropped publish messages | counter/on-event | count | `sys_tree.c:85` |
| 26 | `$SYS/broker/publish/messages/received` | — | PUBLISH received | counter/on-event | count | `sys_tree.c:86` |
| 27 | `$SYS/broker/publish/messages/sent` | — | PUBLISH sent | counter/on-event | count | `sys_tree.c:87` |

## Load-rate topics (`metric_loads[]`, EWMA 1/5/15 min)

Per-second rates, `%.2f`, published only when changed by ≥0.01 (`:161-163`); seeded `0.00` at init (`:188-197`).

`$SYS/broker/load/<X>/{1,5,15}min` for X ∈ {messages/received, messages/sent,
publish/dropped, publish/received, publish/sent, bytes/received, bytes/sent, sockets,
connections} — `sys_tree.c:115-141`. Units: msgs/s, bytes/s, sockets/s, conns/s.

## `$SYS/broker/log/*` (log forwarding, not numeric)

Emitted only when `log_dest topic`; payload = free-text log line; retain=0; expiry 20 s; QoS 2
(`src/logging.c:373-374`). Topics: `/M/subscribe`, `/M/unsubscribe`, `/D`, `/E`, `/W`, `/N`,
`/I`, `/WS` (`src/logging.c:255-313`).

## `$SYS/broker/connection/<remote_clientid>/state` (bridge liveness)

`'1'`/`'0'`, on bridge connect/disconnect, retained + used as Will. `src/bridge.c:328,332,700,701,704,710`.
Gated by `WITH_BRIDGE` + `sys_interval≠0`.

## Tracked-but-NOT-published (enum indices 21-24, 28-51)

Topic and alias are `NULL` in `metrics[]` (`sys_tree.c:81-84,88-112`), so never emitted; the
publish loop and HTTP API skip NULL topics (`sys_tree.c:286`, `http_api.c:249`). Includes the
whole `mqtt/<packet>/{received,sent}` family.

## Gates

| Gate | Kind | Default | Effect |
|---|---|---|---|
| `WITH_SYS_TREE` | compile (`src/CMakeLists.txt:99`, `config.mk:64`) | **ON** | off ⇒ publishing + counters compiled out (`sys_tree.h:22,118-120`); `/systree` → 404 (`http_api.c:259`) |
| `sys_interval` | runtime (`src/conf.c:2628`) | **10 s** | `0` disables all `$SYS` |
| `WITH_MEMORY_TRACKING` | compile (`INC_MEMTRACK`) | **ON** | off ⇒ heap topics NULL |
| `WITH_BRIDGE` | compile | ON | needed for connection-state topics |
| `log_dest topic` | runtime | off | needed for `$SYS/broker/log/*` |

## Caveats for the spec

- Values re-emitted **only on change** — consumers must not assume one publish per interval.
- Several "counter" metrics are sampled from DB state each interval ⇒ they behave as gauges.
- The `$SYS/broker/mqtt/*` family **documented in `man/mosquitto.8.xml:629-770` is not implemented** in this build.
- Dashboard references `…/10min` load topics the broker never publishes (`dashboard/src/app/dashboard.js:850,880`) — dashboard bug.
