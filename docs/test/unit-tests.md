# Unit Tests

## Topic

Unit tests verify one deterministic unit of code without starting the broker,
opening network ports, reading production configuration, or depending on
external services. Use them to make low-level protocol, parsing, data
structure, and error-handling rules fast and precise.

## Technology and location

Use the native C/C++ test tooling already supported by the build:

- CUnit for C unit tests under `test/unit/` (`find_package(CUnit REQUIRED)`).
- GoogleTest and GoogleMock only for C++ mocks and CLI controller tests under
  `test/mock/` and `test/apps/ctrl/`, not for `test/unit/`.
- CMake and CTest for discovery, execution, and reporting.
- Sanitizer runs through the Makefile (`SANITIZER_COMMAND`), not a CMake
  toggle, for memory-safety validation.

Place tests under the matching production boundary:

```text
test/unit/        Pure broker/common/library units
test/lib/         Public client-library and protocol integration
test/broker/      Real broker process and MQTT protocol E2E
```

Do not put process or socket scenarios in `test/unit/`. If a test starts
`mosquitto`, it is an integration or E2E test.

## How to implement

1. Identify one function, parser, data structure, or invariant.
2. Make all inputs explicit.
3. Replace operating-system or network dependencies with narrow fakes only
   when the dependency is not the behavior under test.
4. Assert both the returned result and relevant state changes.
5. Include boundary, invalid-input, and cleanup cases.
6. Register the test in the owning CMake target and run it through CTest.

Prefer table-driven tests for protocol values and error mappings. Keep setup
local to the test unless a fixture expresses a stable domain concept.

## Code example

For a topic-matching unit, keep the test independent of a broker process:

```c
static void topic_match_accepts_single_level_wildcard(void)
{
    bool match = false;

    int rc = mosquitto_topic_matches_sub(
        "devices/+/state",
        "devices/device-17/state",
        &match
    );

    CU_ASSERT_EQUAL(rc, MOSQ_ERR_SUCCESS);
    CU_ASSERT_TRUE(match);
}

static void topic_match_rejects_wrong_depth(void)
{
    bool match = false;

    int rc = mosquitto_topic_matches_sub(
        "devices/+/state",
        "devices/region/device-17/state",
        &match
    );

    CU_ASSERT_EQUAL(rc, MOSQ_ERR_SUCCESS);
    CU_ASSERT_FALSE(match);
}
```

For the repo's C++ targets (`test/mock/`, `test/apps/ctrl/`), use GoogleTest
assertions and GoogleMock only at an external boundary:

```cpp
TEST(PacketParser, RejectsTruncatedVariableHeader)
{
    const std::vector<uint8_t> packet{0x10, 0x01};

    EXPECT_EQ(parse_packet(packet), ParseResult::Malformed);
}
```

## What to cover

- valid and invalid packet fields;
- MQTT wildcard and topic matching;
- property length and encoding boundaries;
- empty, maximum, and overflow-sized inputs;
- state transitions that do not require I/O;
- error-code mapping;
- allocation failure paths where the code supports injection;
- ownership and cleanup of local objects;
- platform-independent utility behavior.

## Best practices

- Keep each test focused on one rule; failures then identify the broken
  contract directly.
- Use deterministic input and fixed expected values; this makes failures
  reproducible.
- Test both sides of every boundary; protocol bugs often occur at zero,
  maximum, and one-over-maximum values.
- Prefer public contracts over private layout assertions; this protects
  refactoring freedom.
- Use a fake with the smallest possible interface; this prevents the test
  double from becoming a second implementation.
- Run unit tests frequently and keep them independent; fast feedback is their
  primary value.

## What to avoid / NOGO

- Do not start a broker, subprocess, socket, or browser in a unit test.
- Do not use real filesystem paths or developer-specific environment state.
- Do not mock the function or algorithm being tested.
- Do not assert incidental log wording when a return code or structured result
  is the contract.
- Do not use sleeps or retries to make deterministic unit tests pass.
- Do not copy an integration fixture into a unit test when a small input value
  expresses the rule more clearly.
