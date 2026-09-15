# Error Handling

## Topic

Represent failure with the project's error codes, preserve the original
failure meaning, and handle errors at the boundary that can make the correct
decision. Never convert a failed security, allocation, protocol, or
persistence operation into success.

## How to implement

For every fallible operation:

1. Validate inputs before mutation or allocation.
2. Call the lower-level operation.
3. Check its return value immediately.
4. Release resources acquired by the current scope.
5. Log actionable context when the boundary can explain it.
6. Return or translate the error deliberately.
7. Let the endpoint choose the protocol response or process exit behavior.

Use `MOSQ_ERR_SUCCESS` only for actual success. Preserve meaningful codes such
as `MOSQ_ERR_INVAL`, `MOSQ_ERR_NOMEM`, `MOSQ_ERR_AUTH`,
`MOSQ_ERR_ACL_DENIED`, `MOSQ_ERR_PROTOCOL`, and `MOSQ_ERR_ERRNO`.

## Code example

Initialization should stop and unwind when a required subsystem fails:

```c
rc = broker_password_file__init();
if(rc != MOSQ_ERR_SUCCESS){
    return rc;
}

rc = broker_acl_file__init();
if(rc != MOSQ_ERR_SUCCESS){
    broker_password_file__cleanup();
    return rc;
}

return MOSQ_ERR_SUCCESS;
```

The security initialization path in `src/security_default.c` demonstrates
fail-fast setup. Similar patterns appear in persistence, listener, TLS, and
plugin initialization.

## Error categories

- **Invalid input:** caller or configuration violates a contract.
- **Protocol error:** received data cannot be processed safely.
- **Authentication/authorization error:** access is denied.
- **Resource error:** allocation, file, socket, or system resource failure.
- **Operational error:** a subsystem cannot start, reload, or persist state.
- **Expected control result:** deferred auth, no subscribers, or a protocol
  condition that is not an implementation failure.

Translate an error only when the receiving boundary needs a different
representation, such as an MQTT reason code or CLI exit code. Keep the
original cause available for logging and diagnostics.

## Best practices

- Check return values immediately; later code may invalidate the cause.
- Unwind in reverse acquisition order.
- Use error codes consistently across public APIs and internal modules.
- Log context once at the most useful boundary.
- Fail closed for security decisions and fail safe for ownership cleanup.
- Make expected negative results distinguishable from infrastructure failure.

## What to avoid / NOGO

- Do not ignore a non-success return value.
- Do not return success after partial initialization.
- Do not use a generic error when a specific project error exists.
- Do not log and continue when the subsystem is unsafe to use.
- Do not use `exit()` deep inside reusable library or broker modules.
