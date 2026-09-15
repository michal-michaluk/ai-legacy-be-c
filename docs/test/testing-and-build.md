# Build, Feature Flags, and Test Contracts

## Topic

Use the build system to define supported feature combinations and use tests to
define observable contracts. System software needs both focused unit tests and
process-level tests that exercise sockets, configuration, authentication,
protocol versions, and platform options.

## How to implement

Keep configuration options named after capabilities (`WITH_TLS`,
`WITH_PERSISTENCE`, `WITH_WEBSOCKETS`, `WITH_TESTS`). For every option:

- define its default;
- express its compile definitions and link dependencies in the owning target;
- provide a supported-off path;
- add a feature-gated test or an explicit test requirement.

Use test layers:

```text
test/unit/       Pure functions and data structures
test/lib/        Client-library protocol and API behavior
test/broker/     Broker behavior over real MQTT packets
test/client/     CLI application behavior
test/apps/       Administrative application behavior
fuzzing/         Parser and boundary robustness
```

Write each integration test as a scenario with setup, a precise expected
packet or return code, and cleanup. Use helper builders for broker
configuration and packet construction so the assertion describes the
behavior, not fixture plumbing.

## Code example

The CMake test setup registers Python scenarios through a shared helper that
adds environment, skip, and resource metadata:

```cmake
add_python_test(
    PY_TEST_NAMES
    ${PREFIX}
    1
    "01-connect-listener-allow-anonymous.py"
)
```

The corresponding Python scenario starts an isolated broker, builds a packet,
and checks the CONNACK result. Create companion cases for wrong credentials,
unsupported features, protocol versions, and reload behavior.

## Best practices

- Test the public behavior at the lowest practical boundary; this catches
  wiring errors that unit tests cannot see.
- Gate tests with `require_features` when a dependency is optional; this
  distinguishes unsupported environments from regressions.
- Keep expected packets and error codes explicit; this preserves protocol
  compatibility.
- Run parallel tests only when resource ownership is declared; this prevents
  port and broker-state races.
- Keep coverage and sanitizers as build targets, not manual tribal knowledge.
- When changing a feature flag, test both enabled and disabled configurations.

## What to avoid / NOGO

- Do not assert private struct layout from an integration test.
- Do not make tests depend on a developer's global broker or fixed external
  ports.
- Do not silently skip a test because setup failed; reserve skips for missing
  declared capabilities.
- Do not add a feature without a failure-path test.
- Do not make a test pass by weakening the production assertion.
