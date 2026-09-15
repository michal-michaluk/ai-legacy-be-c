# Plugin Architecture

## Topic

Extend broker behavior through registered callbacks and explicit plugin
lifecycle contracts. Plugins should add policy or events without duplicating
broker state management.

## How to implement

For a plugin capability:

1. Define the event and callback data required by the extension.
2. Add the public callback contract to the plugin API only when necessary.
3. Register callbacks during plugin initialization.
4. Invoke global and listener-specific callbacks through the existing
   dispatch path.
5. Define the meaning of success, deny, ignore, and defer.
6. Release callback data and plugin resources during cleanup.
7. Keep blocking external work out of the broker thread.

Use callback results deliberately:

```text
MOSQ_ERR_SUCCESS        Explicit allow or successful handling
MOSQ_ERR_ACL_DENIED     Explicit authorization denial
MOSQ_ERR_AUTH           Authentication denial
MOSQ_ERR_PLUGIN_IGNORE  Plugin has no decision
MOSQ_ERR_PLUGIN_DEFER   Decision will be completed elsewhere
```

## Code example

The authentication dispatcher combines global and listener callbacks:

```c
static int plugin__basic_auth(
    struct mosquitto__security_options *opts,
    struct mosquitto *context)
{
    struct mosquitto_evt_basic_auth event_data;
    int final_rc = MOSQ_ERR_PLUGIN_IGNORE;

    DL_FOREACH(opts->plugin_callbacks.basic_auth, callback){
        event_data.client = context;
        event_data.username = context->username;
        event_data.password = context->password;

        int rc = callback->cb(
            MOSQ_EVT_BASIC_AUTH,
            &event_data,
            callback->userdata
        );

        if(rc == MOSQ_ERR_PLUGIN_DEFER){
            final_rc = rc;
        }else if(rc != MOSQ_ERR_PLUGIN_IGNORE){
            return rc;
        }
    }
    return final_rc;
}
```

`src/plugin_basic_auth.c` and `src/plugin_acl_check.c` show the dispatch
pattern. `plugins/password-file` and `plugins/acl-file` show focused provider
implementations.

## Deferred work

Use `MOSQ_ERR_PLUGIN_DEFER` only when the plugin has taken responsibility for
completing the decision. External work may run outside the broker thread, but
completion must re-enter through the documented broker callback or completion
API.

## Best practices

- Keep a plugin focused on one policy or event concern; this makes composition
  predictable.
- Pass the smallest event structure needed; this reduces coupling.
- Register and unregister callbacks symmetrically; this prevents stale
  pointers.
- Treat `IGNORE` and `DEFER` as non-allow decisions; fail closed when no
  component grants access.
- Validate plugin options during initialization and fail startup clearly.
- Version public plugin contracts; external plugins depend on them.

## What to avoid / NOGO

- Do not treat a missing plugin decision as permission.
- Do not make a callback block the broker event loop on network or disk I/O.
- Do not let plugins mutate private broker state without an explicit API.
- Do not retain event-data pointers beyond their documented lifetime.
- Do not add a plugin-specific routing path that bypasses security or expiry.
