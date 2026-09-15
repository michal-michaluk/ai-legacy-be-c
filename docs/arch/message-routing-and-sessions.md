# Message Routing and Sessions

## Topic

Implement message delivery through the broker's shared message store,
subscription matching, client queues, and session lifecycle. Every producer
and consumer must use the same ownership and authorization rules.

## How to implement

For a change to message flow:

1. Identify whether the message is incoming, retained, queued, bridged, or
   plugin-generated.
2. Create or enter the common message-store path.
3. Check topic authorization for the operation.
4. Match subscriptions through the subscription tree.
5. Queue or deliver according to client state, QoS, expiry, and session rules.
6. Track ownership and references until every destination releases the message.
7. Remove expired or disconnected state through the existing lifecycle path.

Use one model for these message sources:

- connected clients;
- retained messages;
- bridge connections;
- plugin-generated messages;
- delayed wills;
- restored persistent messages.

## Code example

The runtime queues plugin messages through the same store used for broker
delivery:

```c
static void queue_plugin_msgs(void)
{
    struct mosquitto__message_v5 *msg, *tmp;

    DL_FOREACH_SAFE(db.plugin_msgs, msg, tmp){
        DL_DELETE(db.plugin_msgs, msg);

        read_message_expiry_interval(&msg->properties, &message_expiry);
        db__messages_easy_queue(
            NULL,
            msg->topic,
            (uint8_t)msg->qos,
            (uint32_t)msg->payloadlen,
            msg->payload,
            msg->retain,
            message_expiry,
            &msg->properties
        );

        mosquitto__message_free(msg);
    }
}
```

When changing this path, preserve expiry handling, property ownership, and
the final free. The routing code in `src/subs.c`, `src/retain.c`, and
`src/database.c` should remain the single source of delivery behavior.

## Sessions and delivery state

Keep these concepts separate:

- clean versus durable session;
- active connection versus stored session;
- retained message versus queued message;
- message store ownership versus per-client queue references;
- source identity versus destination identity.

Session expiry and will-delay processing belong in their existing scheduled
subsystems, not in packet handlers.

## Best practices

- Route every message source through the common store; this keeps QoS,
  expiry, and cleanup consistent.
- Make ownership transfer visible at queue boundaries; this prevents leaks and
  double frees.
- Preserve message metadata such as properties, source identity, expiry, and
  listener information.
- Apply authorization before publishing and before subscription delivery.
- Test disconnected durable clients separately from connected clients.

## What to avoid / NOGO

- Do not directly deliver a plugin or bridge message around subscription
  matching.
- Do not mutate a message after transferring ownership without an explicit
  contract.
- Do not treat retained data as an ordinary client queue.
- Do not let packet handlers implement their own session-expiry logic.
- Do not free a message while references remain in queues or the store.
