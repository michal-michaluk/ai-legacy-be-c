# Agent Guidance

## Document Structure

These documents are universal implementation guidance. Read the document that
matches the change before editing code, and use the source examples as mandatory patterns.

### Architecture implementation guides

Use these task-oriented guides when changing the corresponding area:

| Document | Use when |
| --- | --- |
| [docs/arch/code-structure.md](docs/arch/code-structure.md) | Adding modules, targets, public headers, or platform-specific code |
| [docs/arch/broker-runtime.md](docs/arch/broker-runtime.md) | Changing startup, shutdown, clients, timers, or the event loop |
| [docs/arch/protocol-and-endpoints.md](docs/arch/protocol-and-endpoints.md) | Adding MQTT, WebSocket, HTTP, control-topic, or CLI endpoint behavior |
| [docs/arch/message-routing-and-sessions.md](docs/arch/message-routing-and-sessions.md) | Changing publishing, subscriptions, queues, retained messages, or sessions |
| [docs/arch/plugin-architecture.md](docs/arch/plugin-architecture.md) | Adding or changing plugin callbacks and extensions |
| [docs/arch/security-architecture.md](docs/arch/security-architecture.md) | Changing authentication, authorization, ACLs, TLS identity, or listeners |
| [docs/arch/persistence-and-state.md](docs/arch/persistence-and-state.md) | Changing durable state, persistence records, restore, or SQLite persistence |
| [docs/arch/public-api-and-library.md](docs/arch/public-api-and-library.md) | Changing `libmosquitto`, public headers, ABI, callbacks, or client lifecycle |
| [docs/arch/dashboard-architecture.md](docs/arch/dashboard-architecture.md) | Changing dashboard state, widgets, charts, API data flow, or layout |
| [docs/arch/logging.md](docs/arch/logging.md) | Adding log messages, destinations, severity handling, or logging lifecycle |
| [docs/arch/memory-management.md](docs/arch/memory-management.md) | Adding allocations, ownership transfers, queues, stores, or cleanup paths |
| [docs/arch/feature-flags.md](docs/arch/feature-flags.md) | Adding build options, optional dependencies, compile definitions, or capabilities |
| [docs/arch/error-handling.md](docs/arch/error-handling.md) | Returning, translating, logging, or unwinding errors |
| [docs/arch/lifecycle-management.md](docs/arch/lifecycle-management.md) | Changing startup, reload, shutdown, initialization order, or resource cleanup |

### Testing guides

Use these guides when adding or changing tests:

| Document | Use when |
| --- | --- |
| [docs/test/unit-tests.md](docs/test/unit-tests.md) | Adding or changing isolated C/C++ unit tests |
| [docs/test/broker-e2e-tests.md](docs/test/broker-e2e-tests.md) | Adding or changing broker process and MQTT protocol E2E tests |
| [docs/test/testing-and-build.md](docs/test/testing-and-build.md) | Adding tests, fuzz targets, or test/CI coverage across build options |

## Working Rules

- Preserve the public C API and ABI unless the task explicitly changes them.
- Keep optional capabilities behind their existing build options.
- Return and handle errors explicitly; do not turn a failed security, network,
  or persistence operation into success.
- Add a focused test at the boundary where the changed behavior is observed.
- Avoid unrelated formatting or generated-build changes.
