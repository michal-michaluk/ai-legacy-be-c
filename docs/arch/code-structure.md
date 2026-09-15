# Code Structure

## Topic

Place new code in the narrowest module that owns its responsibility. Preserve
the dependency direction between public API, reusable libraries, broker
runtime, optional applications, plugins, and platform adapters.

## How to implement

Use these boundaries:

```text
include/       Public headers and ABI
common/        Shared protocol-independent helpers
libcommon/     Reusable low-level library
lib/           Client library and wire protocol implementation
src/           Broker runtime and broker-only policy
client/        MQTT command-line clients
apps/          Administrative applications
plugins/       Optional broker extensions
dashboard/     Browser UI
```

Name files after one responsibility:

- `handle_<packet>.c` for packet handling;
- `plugin_<event>.c` for broker event dispatch;
- `<feature>_file.c` for file-backed adapters;
- `mux_<platform>.c` for platform-specific polling;
- `persist_<operation>.c` for persistence operations.

Before adding a file, identify the target that owns it. Add its source,
include directories, compile definitions, and link dependencies to that target
in CMake. Do not solve a target dependency with a global compiler flag.

## Code example

The broker target makes its composition explicit:

```cmake
custom_add_executable(mosquitto
    context.c
    handle_connect.c
    handle_publish.c
    plugin_basic_auth.c
    plugin_acl_check.c
    persist_read.c
    persist_write.c
    mux.c mux_epoll.c mux_kqueue.c mux_poll.c
)

target_link_libraries(mosquitto
    PUBLIC libmosquitto_common
    PRIVATE common-options ${MOSQ_LIBS}
)
```

Use the same pattern for a new module: keep implementation private, expose
only the smallest stable contract, and register the source with its owning
target.

## Best practices

- Keep public headers stable and small; this limits accidental API and ABI
  commitments.
- Keep broker-only state out of `lib/`; this prevents privilege and dependency
  leakage.
- Split protocol handlers by packet responsibility; this keeps lifecycle
  changes reviewable.
- Make optional features explicit in build targets; this makes feature
  combinations reproducible.
- Put platform differences behind one interface; this keeps runtime logic
  portable.

## What to avoid / NOGO

- Do not place new business or security policy in a public header.
- Do not include the broker internal header from unrelated libraries.
- Do not create a catch-all `utils.c` for feature-specific behavior.
- Do not duplicate the same protocol logic in MQTT, HTTP, and WebSocket
  entrypoints.
- Do not modify unrelated generated build artifacts.
