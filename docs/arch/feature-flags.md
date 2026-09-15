# Feature Flags

## Topic

Represent optional capabilities as explicit build or configuration features.
Each feature must have a clear default, dependency rule, enabled path,
disabled path, and observable capability report.

## How to implement

For a new optional capability:

1. Choose a descriptive `WITH_*` or `INC_*` option.
2. Define it once in the top-level CMake configuration.
3. Discover required dependencies only when the feature is enabled.
4. Add compile definitions and linked libraries to the owning target.
5. Guard source code with the same capability contract.
6. Provide a valid disabled build.
7. Report the capability when runtime diagnostics already report features.
8. Add tests that require the feature and tests for its disabled behavior.

## Code example

The build uses a helper to expose options to both CMake and test
environments:

```cmake
function(option_env name description value)
    option(${name} "${description}" ${value})
    get_property(test_vars GLOBAL PROPERTY TEST_ENV_VARS)
    set_property(
        GLOBAL PROPERTY TEST_ENV_VARS
        "${test_vars};${name}=${${name}}"
    )
endfunction()

option_env(WITH_TLS "Include SSL/TLS support?" ON)
option_env(WITH_WEBSOCKETS "Include websockets support?" ON)
option_env(WITH_PERSISTENCE "Include persistence support?" ON)
```

The owning target then adds definitions and libraries conditionally:

```cmake
if(WITH_TLS)
    find_package(OpenSSL REQUIRED)
    target_compile_definitions(mosquitto PRIVATE WITH_TLS)
    target_link_libraries(mosquitto PRIVATE OpenSSL::SSL)
endif()
```

## Compile-time versus runtime options

- Use a **build flag** when code or dependencies should not be included.
- Use a **configuration option** when one binary should support different
  deployments.
- Use an **environment variable** only for narrowly scoped diagnostics or
  operational overrides with an explicit security policy.

Do not duplicate a feature flag in multiple build systems without documenting
how the values correspond.

## Best practices

- Make defaults conservative and compatible with supported installations.
- Fail configuration clearly when an explicitly enabled dependency is missing.
- Keep feature-specific includes and link libraries inside the feature branch.
- Make disabled behavior explicit rather than relying on accidental absence.
- Report enabled capabilities through existing startup diagnostics.
- Test important combinations, especially TLS, persistence, WebSockets, and
  fuzzing builds.

## What to avoid / NOGO

- Do not scatter undocumented `#ifdef` checks through unrelated modules.
- Do not silently disable a feature requested by the user.
- Do not link optional libraries unconditionally.
- Do not add a flag for a behavior that should be ordinary configuration.
- Do not make tests depend on a feature without declaring the requirement.
