# Public API and Client Library

## Topic

Extend the client library through stable public contracts while keeping
networking, packet state, and platform details private. Treat installed
headers and exported symbols as compatibility commitments.

## How to implement

For a public API change:

1. Define the caller-visible behavior and ownership contract.
2. Add the smallest declaration to the appropriate public header.
3. Implement it in `lib/` using existing lifecycle and error conventions.
4. Preserve synchronous and threaded-loop semantics.
5. Update symbol visibility or linker maps when required.
6. Document thread-safety, callback timing, buffer ownership, and errors.
7. Add client-library coverage at the public boundary.

Keep this dependency direction:

```text
application
    -> public include/
    -> libmosquitto API
    -> private packet/network implementation
    -> common libraries and platform services
```

## Code example

A public operation should validate its inputs, use the client lifecycle, and
return the established error code:

```c
int mosquitto_publish(
    struct mosquitto *mosq,
    int *mid,
    const char *topic,
    int payloadlen,
    const void *payload,
    int qos,
    bool retain)
{
    if(mosq == NULL || topic == NULL || payloadlen < 0){
        return MOSQ_ERR_INVAL;
    }
    if(qos < 0 || qos > 2){
        return MOSQ_ERR_QOS_NOT_SUPPORTED;
    }

    return send__publish(mosq, mid, topic, payloadlen, payload, qos, retain);
}
```

The public API in `include/`, implementation in `lib/`, and C++ wrapper in
`lib/cpp/` demonstrate the facade pattern. The application should not need to
know packet layout or socket state.

## Compatibility concerns

Preserve:

- existing function signatures;
- error-code meanings;
- callback order and callback lifetime;
- caller-owned versus library-owned buffers;
- thread-safety guarantees;
- shared-library symbol names;
- static-library build behavior.

Use linker maps such as `lib/linker.version` to control exported symbols.
Avoid exposing internal helpers merely to reuse them from an application.

## Best practices

- Prefer additive APIs over changing existing parameter semantics.
- Use opaque handles and documented lifecycle functions for mutable state.
- Validate input at the public boundary and return a defined library error.
- Document whether data is copied, borrowed, or retained asynchronously.
- Keep public headers independent of broker-private structures.
- Preserve behavior across MQTT protocol versions where the API promises it.

## What to avoid / NOGO

- Do not expose `mosquitto_broker_internal.h` types in the client API.
- Do not change an existing error code to mean something else.
- Do not retain caller memory past the documented lifetime.
- Do not make a previously non-blocking operation perform hidden blocking I/O.
- Do not export private symbols as a shortcut to cross-module access.
