# Logging

## Topic

Use one logging subsystem with explicit severity, destinations, initialization,
and shutdown. Feature code should report operational facts through the shared
logger instead of writing directly to stdout, stderr, or files.

## How to implement

When adding a log message:

1. Choose the severity that matches operator actionability.
2. Include the relevant client, listener, topic, or subsystem identity.
3. Avoid credentials, payloads, private keys, and unbounded user input.
4. Use the shared logging function.
5. Return the underlying error separately; logs are not error propagation.
6. Ensure the message remains useful across configured destinations.

The broker initializes destinations from configuration, opens optional files,
and closes them during shutdown. Follow the same lifecycle for a new
destination or sink.

## Code example

Runtime code reports a reason while preserving the protocol error separately:

```c
if(rc == MOSQ_ERR_AUTH){
    log__printf(
        NULL,
        MOSQ_LOG_NOTICE,
        "Client %s [%s:%d] disconnected: not authorised.",
        context->id,
        context->address,
        context->remote_port
    );
    return rc;
}
```

The central implementation in `src/logging.c` maps severity and destination
flags to stderr, stdout, files, syslog, `$SYS` topics, DLT, or Android
logging. `log__init()` and `log__close()` own sink lifecycle.

## Severity guidance

- `MOSQ_LOG_ERR`: operation failed or service cannot continue.
- `MOSQ_LOG_WARNING`: unsafe or degraded behavior needs attention.
- `MOSQ_LOG_NOTICE`: security, connection, or configuration event.
- `MOSQ_LOG_INFO`: normal lifecycle and capability information.
- `MOSQ_LOG_DEBUG`: diagnostic detail, disabled or filtered in normal use.
- `MOSQ_LOG_INTERNAL`: implementation diagnostics, not user-facing events.

## Best practices

- Use stable severity semantics; operators and automation depend on them.
- Include identifiers that make a failure actionable without exposing secrets.
- Log configuration fallback and degraded behavior explicitly.
- Keep formatting in the logger so timestamps and destinations stay
  consistent.
- Log once at the boundary that can explain the failure; avoid duplicate noise.
- Make reload and shutdown logging reflect the actual lifecycle order.

## What to avoid / NOGO

- Do not call `printf`, `fprintf`, or `perror` from broker feature code.
- Do not log passwords, tokens, private keys, or full message payloads.
- Do not use `MOSQ_LOG_ERR` for an expected client rejection.
- Do not treat successful logging as proof that an operation succeeded.
- Do not add a destination-specific branch to every feature module.
