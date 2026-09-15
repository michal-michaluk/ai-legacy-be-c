# Persistence and State

## Topic

Persist only broker state that must survive restart, and make serialization
consistent with in-memory ownership, expiry, protocol properties, and restore
behavior.

## How to implement

For a new durable field:

1. Decide whether it is runtime-only or restart-critical.
2. Add it to the in-memory owner.
3. Define a versioned persistence representation.
4. Write the representation with explicit lengths and ownership rules.
5. Read it symmetrically and validate bounds before allocation.
6. Define behavior for missing, corrupt, old, and partially written data.
7. Add cleanup for both successful and failed restore paths.

The persistence boundary should serialize values, not process-local pointers.
Persist identifiers, timestamps, protocol fields, properties, and payloads
that are required to reconstruct behavior.

## Code example

The message-store writer persists metadata needed for delivery semantics:

```c
chunk.F.store_id = base_msg->data.store_id;
chunk.F.expiry_time = base_msg->data.expiry_time;
chunk.F.payloadlen = base_msg->data.payloadlen;
chunk.F.topic_len = (uint16_t)strlen(base_msg->data.topic);
chunk.F.qos = base_msg->data.qos;
chunk.topic = base_msg->data.topic;
chunk.payload = base_msg->data.payload;
chunk.properties = base_msg->data.properties;

int rc = persist__chunk_message_store_write_v6(db_fptr, &chunk);
if(rc){
    return rc;
}
```

`src/persist_write.c` and `src/persist_read_v5.c` demonstrate the required
write/read symmetry. Preserve the distinction between retained messages,
queued client messages, durable clients, expiry, and `$SYS` data.

## State categories

- **Ephemeral:** active sockets, event-poller handles, callback pointers.
- **Runtime:** connected client contexts, subscription indexes, message
  references, timers.
- **Durable:** non-clean sessions, queued messages, retained messages, expiry
  metadata, required message properties.
- **Configuration:** listeners, plugins, credentials, ACL files, persistence
  options.

Optional persistence backends such as `plugins/persist-sqlite` must preserve
the same durable-state contract as the built-in persistence implementation.

## Best practices

- Version persistence records; upgrades need an explicit compatibility path.
- Validate lengths before allocation; persisted data is not trusted input.
- Persist expiry timestamps and restore them using current time semantics.
- Keep write and read ownership explicit; never serialize raw pointers.
- Make restore failure visible and safe; do not start with silently incomplete
  durable state.
- Avoid persisting transient `$SYS` values as retained business data.

## What to avoid / NOGO

- Do not add a field to the in-memory model without deciding its durability.
- Do not write unbounded strings or payload lengths without validation.
- Do not persist platform handles, pointers, mutexes, or event-loop state.
- Do not treat a successful write of only the payload as a successful state
  snapshot.
- Do not ignore partial, corrupt, or incompatible persistence records.
