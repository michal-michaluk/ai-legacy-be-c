# Broker Runtime

## Topic

Modify the broker runtime through explicit lifecycle functions and one event
loop. The loop orchestrates work; packet handlers, queues, timers, and policy
modules own their behavior.

## How to implement

For a runtime feature:

1. Identify its lifecycle: initialize, use, reload if applicable, and cleanup.
2. Store state in the owning broker or client structure.
3. Add one focused function for each lifecycle operation.
4. Call the operation from the main loop or lifecycle boundary.
5. Return errors to the caller; do not continue with partial initialization.
6. Ensure shutdown releases processes, sockets, memory, and callbacks.

The main loop should process scheduled broker work before blocking in the
platform multiplexer:

```c
while(g_run){
    retain__expiry_check();
    queue_plugin_msgs();
    context__free_disused();
    keepalive__check();
    session_expiry__check();
    will_delay__check();

    int rc = mux__handle(listensock, listensock_count);
    if(rc != MOSQ_ERR_SUCCESS){
        return rc;
    }
}
```

Add a new periodic action as a named subsystem function. Update the next
deadline when the action has a future wake-up time. Do not add feature logic
directly to the loop body.

## Runtime boundaries

- `mosquitto.c`: process startup and shutdown.
- `loop.c`: orchestration and scheduled work.
- `context.c`: client context lifecycle.
- `listeners.c`: listener setup and teardown.
- `mux*.c`: platform event polling.
- `signals.c`: process-level signal handling.

## Best practices

- Keep the loop readable as a sequence of responsibilities; this makes
  scheduling and failure behavior auditable.
- Make initialization order explicit; dependencies must be ready before use.
- Use the broker's allocator and cleanup conventions consistently.
- Keep blocking I/O out of the main loop; it stalls every connected client.
- Reconcile active clients when configuration or security policy changes.
- Make repeated timer work idempotent; event loops can revisit a condition.

## What to avoid / NOGO

- Do not block on disk, DNS, external authentication, or long computation in
  the main loop.
- Do not add hidden global state when the state belongs to a client, listener,
  or broker configuration.
- Do not swallow subsystem errors and continue as if the operation succeeded.
- Do not free client state while it is still reachable from a queue or hash.
- Do not duplicate platform polling logic in protocol handlers.
