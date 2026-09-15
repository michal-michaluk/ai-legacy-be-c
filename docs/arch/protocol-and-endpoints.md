# Protocol and Endpoints

## Topic

Add external behavior through an endpoint adapter that translates transport
input into broker operations. Keep transport parsing, protocol validation,
broker policy, and response encoding separate.

## How to implement

For a new endpoint or protocol operation:

1. Define the public request and response contract.
2. Locate the transport adapter that accepts the request.
3. Decode and validate input at the protocol boundary.
4. Call shared broker logic rather than duplicating state changes.
5. Apply authentication and authorization before side effects.
6. Encode protocol-specific success and error responses.
7. Preserve MQTT version and transport differences at the edge.

The main endpoint families are:

```text
MQTT TCP          Binary MQTT packets over listeners
MQTT TLS          MQTT over encrypted sockets
MQTT WebSockets   MQTT frames over WebSockets
HTTP API          Operational HTTP requests
$CONTROL topics   MQTT management commands
CLI applications  Process and argument interfaces
```

## Code example

A packet handler should validate, delegate, and respond:

```c
int handle_publish(struct mosquitto *context)
{
    struct mosquitto__message_v5 message;

    int rc = packet__decode_publish(context, &message);
    if(rc != MOSQ_ERR_SUCCESS){
        return send_publish_error(context, rc);
    }

    rc = mosquitto_acl_check(
        context,
        message.topic,
        message.payloadlen,
        message.payload,
        message.qos,
        message.retain,
        message.properties,
        MOSQ_ACL_WRITE
    );
    if(rc != MOSQ_ERR_SUCCESS){
        return send_publish_error(context, rc);
    }

    return db__message_store_and_route(context, &message);
}
```

The concrete handlers in `src/handle_connect.c`, `src/handle_publish.c`, and
`src/handle_subscribe.c` demonstrate this boundary. HTTP and WebSocket
adapters should call equivalent broker operations rather than maintain a
second message model.

## Best practices

- Validate untrusted input once at the boundary and pass typed data inward.
- Keep protocol error codes and response formatting at the endpoint edge.
- Reuse authorization and routing logic across transports; this prevents
  security drift.
- Support protocol-version differences explicitly; do not infer behavior from
  packet length alone.
- Keep endpoint handlers short enough to show the request lifecycle.

## What to avoid / NOGO

- Do not let an HTTP or WebSocket handler bypass MQTT authorization rules.
- Do not mutate broker state before request validation completes.
- Do not duplicate publish, subscription, or session logic per transport.
- Do not return success after a response write or broker mutation fails.
- Do not expose private broker structures as an endpoint response contract.
