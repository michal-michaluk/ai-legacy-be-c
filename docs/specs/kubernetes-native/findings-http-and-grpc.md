# Findings — HTTP server & gRPC presence in current code

Answers: "is an HTTP server included?" and "is gRPC present?". Paths relative to `mosquitto/`.

## HTTP server — included in source: YES (but compiled out locally)

Mosquitto ships a **libmicrohttpd-based HTTP API** (the "HTTP server" in current code):

| Aspect | Evidence |
|---|---|
| Implementation | `src/http_api.c` (`#ifdef WITH_HTTP_API` guards whole file) |
| Build option (make) | `config.mk:150-151` — `WITH_HTTP_API=yes` (**default ON**) |
| Make definition + link | `make/broker.mk:23-25` — `-DWITH_HTTP_API`, `-lmicrohttpd` |
| CMake discovery + wiring | `src/CMakeLists.txt:148-166` — `pkg_check_modules(libmicrohttpd)`/`find_library`, `target_sources(http_api.c)`, `target_compile_definitions`, `target_link_libraries`; **warns and disables if not found** |
| Packaged dependency | `vcpkg.json` lists `libmicrohttpd` |
| Listener protocol | `src/conf.c:2455-2457` — `protocol http_api` |
| Lifecycle wiring | `src/listeners.c:247` (`http_api__start_local`), `:298-299` (start), `:331` (stop) |
| Data exposed | `src/http_api.c:242-260` — `GET /systree` (JSON of `$SYS` metrics) |

**Local-build nuance:** `libmicrohttpd` is **not installed on the host** (no brew package).
The CMake configure log/cache shows `libmicrohttpd_FOUND:INTERNAL=` (empty) ⇒ the
`else → message(WARNING "microhttpd not found, disabling WITH_HTTP_API")` branch ran.

Proof in the built artifact `build/src/mosquitto`:
- `otool -L` → links only `libssl`, `libcrypto`, `libcjson`, `libSystem` — **no microhttpd**.
- No `http_api__*` or `MHD_*` symbols; only unrelated websockets helpers
  (`_http__read`, `_http__context_init`, `_parse_http_version` from picohttpparser).
- `strings` shows no `/systree`.

⇒ **The HTTP API exists in source and defaults ON, but the current local broker binary has it compiled out.** The dashboard is a **separate static UI** (`dashboard/src`) with no server — served by `python3 -m http.server` (see `.agents/skills/run/SKILL.md`).

## gRPC — NOT present

- `grep -rniE "grpc|protobuf|protoc"` over the whole tree (source, `CMakeLists.txt`, `*.mk`,
  `vcpkg.json`, `deps/`, `build*`) → **zero** real hits (only the substring "protocol").
- `deps/` contains only `picohttpparser/`, `uthash.h`, `utlist.h`.
- `vcpkg.json` dependencies: `argon2, cjson, cunit, dlfcn-win32, gtest, libmicrohttpd, openssl, pthreads, sqlite3` — **no grpc, no protobuf**.
- No OpenTelemetry anywhere (confirms F3).

## Implications for C1 / C2

- **C1 Prometheus (pull, HTTP):** the microhttpd HTTP-API pattern is the ready-made template
  (`http_api.c` + listener protocol + CMake discovery). Adding a `/metrics` handler reuses it —
  **but** the dependency must actually be present at build time (it is not, locally). An
  alternative HTTP server could avoid microhttpd, at the cost of a new dependency.
- **C2 OTLP gRPC:** fully greenfield — no gRPC/protobuf anywhere ⇒ a new heavy dependency (confirms R3).
- **C2 OTLP HTTP:** needs an **HTTP client** to push to the collector (microhttpd is a *server*);
  no HTTP client library (e.g. libcurl) is currently a dependency ⇒ also new.
- **Websockets** already parse the HTTP upgrade (`picohttpparser`, `src/websockets.c`) — unrelated to the metrics endpoint.
