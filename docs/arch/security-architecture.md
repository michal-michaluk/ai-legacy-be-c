# Security Architecture

## Topic

Implement authentication and authorization as separate, fail-closed stages.
Authenticate the client identity first, then authorize every operation
against listener policy, identity, topic rules, and configured plugins.

## How to implement

Use this sequence:

1. Select global and listener-scoped security options.
2. Authenticate username/password, certificate identity, or PSK identity.
3. Reject malformed or unsafe identities.
4. Authorize CONNECT, publish, subscribe, and unsubscribe operations.
5. Combine global and listener plugin decisions.
6. Reapply policy after configuration reload.
7. Disconnect active clients that are no longer authorized.

Keep authentication providers separate from ACL providers. A password-file
provider verifies credentials; an ACL-file provider decides topic access.

## Code example

The basic-auth flow does not turn an absent plugin decision into a grant:

```c
int mosquitto_basic_auth(struct mosquitto *context)
{
    bool plugin_used = false;
    int rc = run_global_auth_callbacks(context, &plugin_used);
    if(rc != MOSQ_ERR_SUCCESS && rc != MOSQ_ERR_PLUGIN_IGNORE){
        return rc;
    }

    rc = run_listener_auth_callbacks(context, &plugin_used);
    if(rc != MOSQ_ERR_SUCCESS && rc != MOSQ_ERR_PLUGIN_IGNORE){
        return rc;
    }

    if(context->username == NULL && effective_allow_anonymous(context)){
        return MOSQ_ERR_SUCCESS;
    }
    return MOSQ_ERR_AUTH;
}
```

The ACL path in `src/plugin_acl_check.c` checks special topics, invokes global
and listener callbacks, and converts an unresolved deferred result into
denial. The ACL provider in `plugins/acl-file/acl_check.c` checks explicit
denials before allows and rejects unsafe wildcard identities.

## Identity and topic safety

When mapping a TLS certificate to a username:

- use only a validated client certificate;
- reject missing certificate identity;
- reject embedded NUL characters;
- reject MQTT wildcard characters before topic-pattern substitution;
- do not log private credentials or key material.

Apply the same validation to usernames and client IDs used in ACL patterns.

## Best practices

- Default to deny; this prevents plugin omissions from becoming access grants.
- Keep `allow_anonymous` listener-scoped where trust zones differ.
- Authorize every publish and subscription operation, not just CONNECT.
- Recheck active sessions after reload; policy changes must take effect.
- Keep credential verification and topic authorization in separate modules.
- Test explicit allow, explicit deny, missing credentials, invalid identity,
  listener overrides, and reload behavior.

## What to avoid / NOGO

- Do not treat `IGNORE` or `DEFER` as success.
- Do not authorize only at connection time.
- Do not interpolate unvalidated client IDs or usernames into ACL topics.
- Do not log passwords, private keys, or complete credential records.
- Do not use anonymous access as a fallback for failed authentication.
