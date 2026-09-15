# Findings — current logging architecture (for JSON logging)

Evidence for the structured-logging capability. Paths relative to `mosquitto/`.

## Core implementation

Single file `src/logging.c`.

| Function | Location | Role |
|---|---|---|
| `log__init` | `src/logging.c:114` | copy `config->log_type`→`log_priorities`, `log_dest`→`log_destinations`; open syslog/file/DLT |
| `log__close` | `src/logging.c:159` | teardown |
| `log__vprintf` | `src/logging.c:234` | **single choke point**: filtering + formatting + destination fan-out |
| `log__printf` | `src/logging.c:392` | varargs wrapper → `log__vprintf` |
| `log__internal` | `src/logging.c:407` | 200-byte buffer + ANSI, → `log__printf` |
| `mosquitto_log_vprintf` / `mosquitto_log_printf` | `src/logging.c:430,436` | public plugin/library entries |
| `libcommon__vprintf` | `src/logging.c:446` | routes libcommon → `log__vprintf(INFO,…)` |

Static state: `log_destinations` default `MQTT3_LOG_STDERR` (`:67`); `log_priorities` default
`ERR|WARNING|NOTICE|INFO` (`:68`); file buffer `log_fptr_buffer[BUFSIZ]` (`:48`).

## Destinations

Bitmask — `src/mosquitto_broker_internal.h:55-66`: `NONE, SYSLOG, FILE, STDOUT, STDERR, TOPIC,
DLT, ANDROID, ALL`. Fan-out in `log__vprintf`: stdout `:352`, stderr `:355`, file `:358`,
syslog `:365`, topic `:373` (skips DEBUG/INTERNAL), DLT `:377`, android `:382`.

Config parsing: `log_dest` `src/conf.c:2066-2114`; `log_type` `:2169-2200`; defaults `:317-336`.

## Line format

Composed in `log__vprintf` `:336-350` into a fixed **1000-byte** stack buffer (`:237`):

```
[<timestamp>: ]<formatted message>\n
```

- timestamp from `log_timestamp` (default true, `src/conf.c:332`): `strftime(log_timestamp_format,…)`
  when format set (`:337-341`), else raw epoch `%PRIu64` (`:343`), then `": "`.
- message via `vsnprintf(&log_line[pos], …)` (`:350`).
- **No structured field, no level prefix, no hostname/PID in the line** (PID goes to syslog via `openlog` `:123`).

## Levels & filtering

Constants `include/mosquitto/defs.h:34-45`: `NONE, INFO, NOTICE, WARNING, ERR, DEBUG, SUBSCRIBE,
UNSUBSCRIBE, WEBSOCKETS, INTERNAL(0x80000000), ALL`.

Filter: `if((log_priorities & priority) && log_destinations != MQTT3_LOG_NONE)` (`src/logging.c:253`).

## Impact for a JSON formatter (R1)

- **One choke point** (`log__vprintf`, plus `log__internal`) — output can be reformatted without touching emit sites.
- **But ~636 `log__printf` call sites across 50 files** in `src/`, all passing **pre-rendered human
  strings** (e.g. `"Received PUBLISH from %s (d%d, q%d, …)"`). True structured fields require
  enriching/replacing call sites.
- `log__internal`: 2 sites. Public `mosquitto_log_printf`/`_vprintf`: no in-tree callers.
- `log__init`/`log__close` are called on config reload (`src/signals.c:104-105,126-127`) — a JSON logger must reinit there.
- `config->log_fptr` flows via `db.config` (`src/logging.c:246-249`).
- Client library has a **separate** logger (`lib/logging_mosq.c`) — unrelated to broker logging.
