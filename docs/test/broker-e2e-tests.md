# Broker End-to-End Tests

## Topic

Broker E2E tests validate the assembled service through its public boundaries:
process startup, configuration, listeners, MQTT packets, authentication,
authorization, persistence, logs, and shutdown. They should use a real broker
process and an external test client rather than calling broker internals.

## Technology and location

Use the established stack:

- Python 3 test scenarios.
- CTest for registration and execution.
- `test/mosq_test.py` for sockets, process startup, readiness checks, logs,
  retries, and cleanup.
- `test/mosquitto_broker.py` for context-managed broker lifecycle.
- `test/broker_config.py` for typed configuration builders.
- `test/mqtt_packets.py` for deterministic MQTT packet construction.
- `.conf`, password, ACL, TLS, and persistence fixtures beside scenarios.

Put broker scenarios under `test/broker/`. Use `test/lib/` when the client
library itself is the primary subject, and `test/apps/` when a command-line
application is the primary subject.

## How to implement

1. Define the externally observable behavior.
2. Allocate an isolated port or declare the required CTest resource count.
3. Build a minimal broker configuration with `BrokerConfig`,
   `ListenerConfig`, or a scenario fixture.
4. Start the real broker and wait for listener readiness.
5. Connect through MQTT, HTTP, or WebSocket as appropriate.
6. Assert exact packets, status codes, payloads, side effects, or exit codes.
7. Exercise cleanup and inspect logs on failure.
8. Stop the broker and remove generated files even when assertions fail.

The normal flow is:

```text
Python scenario
  -> generated config and fixtures
  -> real mosquitto subprocess
  -> readiness probe
  -> external protocol client
  -> exact observable assertion
  -> controlled shutdown
```

## Code example

This pattern uses the repository's broker wrapper and packet helpers:

```python
from broker_config import BrokerConfig, ListenerConfig
from mosquitto_broker import MosquittoBroker
import mosq_test
import mqtt_packets

port = mosq_test.get_port()
config = BrokerConfig(
    listeners=[ListenerConfig(port=port)],
    allow_anonymous=False,
    password_file="connect-password.pwfile",
)

connect = mqtt_packets.gen_connect(
    "connect-password-test",
    username="user",
    password="correct-password",
    proto_ver=5,
)
expected = mqtt_packets.gen_connack(rc=0, proto_ver=5)

with MosquittoBroker(config=config) as broker:
    sock = mosq_test.do_client_connect(
        connect,
        expected,
        port=broker.port,
    )
    sock.close()
```

Create companion scenarios for wrong credentials, anonymous access, protocol
versions, TLS requirements, explicit ACL denial, configuration reload, and
broker startup failure. Keep each scenario's `.conf`, `.pwfile`, `.acl`, or
certificate fixture beside the Python file when a static fixture improves
readability.

## Protocol and endpoint assertions

Use the driver that matches the public endpoint:

| Endpoint | Driver | Assertions |
| --- | --- | --- |
| MQTT TCP | Raw socket and `mqtt_packets.py` | Packet bytes, reason codes, properties, message ordering |
| MQTT over TLS | Python SSL socket helpers | Handshake, certificate policy, MQTT behavior |
| WebSockets | WebSocket-capable client | Upgrade, frame transport, MQTT behavior |
| HTTP API | HTTP client against a real broker | Status, headers, JSON/body, authorization, side effects |
| CLI application | Python subprocess | Exit code, stdout, stderr, stdin, filesystem effect |

Do not assert only that the process stayed alive. Assert the contract visible
to the client and the resulting broker state.

## Isolation and failure handling

Use dynamically allocated ports and resource declarations for parallel tests.
Do not use a developer's running broker. The harness should capture broker
logs, report them when a test fails, and terminate the owned process.

Use feature gates such as `require_features(["WITH_TLS"])` when the test
requires an optional build capability. A missing declared capability may skip
the test; a broken setup must fail the test.

For expected startup failures, assert both the non-zero outcome and the
diagnostic that explains the invalid configuration. Avoid broad log matching
when a structured protocol or exit-code assertion is available.

## Best practices

- Test through the same boundary real users use; this catches build and wiring
  errors that unit tests cannot detect.
- Keep each scenario focused on one behavior; this makes packet failures
  diagnosable.
- Use exact protocol assertions; compatibility depends on reason codes,
  properties, flags, and ordering.
- Run both MQTT v3 and v5 where behavior differs; protocol compatibility is
  part of the contract.
- Include negative paths: malformed input, denied access, missing files,
  failed TLS, reload failure, and persistence corruption.
- Make cleanup unconditional; leaked brokers and ports make later failures
  misleading.
- Declare resource requirements before enabling parallel execution; this
  prevents race-driven flakiness.

## What to avoid / NOGO

- Do not call private broker functions instead of testing the public boundary.
- Do not use fixed ports, global configuration files, or a shared live broker.
- Do not retry deterministic assertions; retry only process readiness or
  explicitly eventual behavior.
- Do not silently skip because a fixture, executable, or configuration is
  missing.
- Do not make an E2E test depend on remote services or current wall-clock
  time.
- Do not leave generated configs, credentials, certificates, processes, or
  sockets behind after the test.
