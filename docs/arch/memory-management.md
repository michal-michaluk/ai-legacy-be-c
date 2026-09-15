# Memory Management

## Topic

Make allocation, ownership transfer, and release explicit for every heap
object. Use the project's allocator wrappers and cleanup conventions so
memory tracking, alternate allocators, and failure paths remain consistent.

## How to implement

For every allocated object, define:

1. The owner immediately after allocation.
2. Whether the object is borrowed, transferred, or reference-counted.
3. The cleanup function and its valid call states.
4. The behavior when a later allocation fails.
5. The point at which ownership moves into a queue, store, or client.

Use the common allocator wrappers rather than raw allocation calls. Keep
partial initialization cleanup next to the initialization sequence.

## Code example

The message store pattern allocates, validates, transfers, and cleans up:

```c
struct mosquitto__base_msg *base_msg;

base_msg = mosquitto_calloc(1, sizeof(*base_msg));
if(base_msg == NULL){
    return MOSQ_ERR_NOMEM;
}

base_msg->data.payload = mosquitto_malloc(payloadlen + 1);
if(base_msg->data.payload == NULL){
    db__msg_store_free(base_msg);
    return MOSQ_ERR_NOMEM;
}

memcpy(base_msg->data.payload, payload, payloadlen);
base_msg->data.payload[payloadlen] = '\0';

if(db__message_store(context, base_msg, expiry)){
    db__msg_store_free(base_msg);
    return MOSQ_ERR_NOMEM;
}
```

Use `libcommon/memory_common.c` and `include/mosquitto/libcommon_memory.h`
as the allocator boundary. Follow the ownership rules in `database.c`,
`retain.c`, and persistence code when an object is shared.

## Ownership patterns

- **Owned object:** one module allocates and frees it.
- **Transferred object:** the callee becomes responsible after a successful
  call.
- **Borrowed pointer:** valid only for the documented call or scope.
- **Reference-counted message:** queues and clients hold references until
  delivery or removal.
- **Configuration-owned string:** freed with the configuration lifecycle.

Document transfers in function names, comments, or API contracts when the
ownership is not obvious.

## Best practices

- Allocate with zero initialization when invariants require empty fields.
- Check every allocation before dereferencing or transferring it.
- Clean up in reverse order when initialization is partially successful.
- Centralize complex object destruction in one function.
- Free detached linked-list nodes while iterating with a safe temporary.
- Use sanitizers and memory tracking to validate lifecycle changes.

## What to avoid / NOGO

- Do not mix raw `malloc/free` with project allocator wrappers.
- Do not free a pointer after ownership has been transferred.
- Do not return a partially initialized object as if it were valid.
- Do not store a borrowed pointer in a long-lived queue.
- Do not hide ownership changes in unrelated helper functions.
