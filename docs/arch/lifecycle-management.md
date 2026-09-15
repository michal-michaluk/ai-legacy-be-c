# Lifecycle Management

## Topic

Treat startup, reload, steady state, shutdown, and cleanup as explicit
phases. Every resource acquired in one phase must have a matching release
path, including error and signal paths.

## How to implement

Model a service lifecycle as:

```text
parse configuration
  -> validate configuration
  -> initialize logging and resources
  -> initialize security, plugins, persistence, and listeners
  -> run event loop
  -> process shutdown work
  -> stop listeners and plugins
  -> close persistence and logging
```

For a new subsystem:

1. Add initialization after its prerequisites are ready.
2. Add validation before accepting traffic.
3. Define reload behavior or explicitly reject reload.
4. Define shutdown ordering and ownership.
5. Add cleanup for partial initialization.
6. Ensure repeated cleanup is safe where signals or startup failures can
   re-enter the path.

## Code example

The broker performs shutdown work before closing infrastructure:

```c
static void post_shutdown_cleanup(void)
{
    HASH_ITER(hh_id, db.contexts_by_id, context, context_tmp){
        context__send_will(context);
    }
    will_delay__send_all();
    session_expiry__add_on_shutdown();

    db.shutdown = true;

    broker_control__cleanup();

#ifdef WITH_PERSISTENCE
    persist__backup(true);
#endif

    session_expiry__remove_all();
    listeners__stop();
    log__close(db.config);
}
```

The exact functions vary by subsystem, but the ordering rule is important:
flush protocol-visible state and durable state before destroying listeners,
plugins, stores, or logging sinks.

## Reload management

Reload is not a second startup. It must define which resources can be
reconfigured in place and which require replacement. On reload:

- parse and validate the new configuration first;
- preserve the old valid configuration until the new one is ready;
- reinitialize logging or TLS only after validation;
- reapply security to active clients;
- close replaced resources;
- report partial or rejected changes explicitly.

## Best practices

- Keep lifecycle functions named and discoverable: `*_init`, `*_cleanup`,
  `*_reload`, `*_stop`, or equivalent.
- Initialize dependencies before dependents and tear them down in reverse.
- Make shutdown idempotent where signals and startup failures can overlap.
- Flush retained, queued, will, and persistence state before freeing clients.
- Make abnormal termination visible in logs and return codes.
- Keep resource ownership with the subsystem that acquired it.

## What to avoid / NOGO

- Do not accept traffic before security and listeners are fully initialized.
- Do not close logging before final failure diagnostics are emitted.
- Do not destroy a store before queues and client references are drained.
- Do not replace live configuration with an unvalidated partial configuration.
- Do not assume only the normal shutdown path runs.
