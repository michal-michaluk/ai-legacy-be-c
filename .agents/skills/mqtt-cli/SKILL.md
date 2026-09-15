---
name: mqtt-cli
description: Ad-hoc MQTT messaging against local, k3s or remote brokers using the three client CLIs (mosquitto_pub / mosquitto_sub / mosquitto_rr). Use to publish, subscribe, or request/response for agentic MQTT tests. Listening is non-blocking and captures to a file.
---

# mqtt-cli

Thin, env-aware wrapper over the broker's own client CLIs — no connection flags
to remember, just an env name. Non-blocking listeners capture to a file so the
agent session never hangs; `pub` and `rr` run blocking.

Profiles: `.agents/mqtt-cli.yaml` · Wrapper: `scripts/mqtt.py`

```sh
M=.agents/skills/mqtt-cli/scripts/mqtt.py
```

## Environments

```sh
python3 $M envs            # list profiles (* = default)
python3 $M show k3s        # resolved connection + client flags
```

`local` = the native broker from the run skill; `k3s` = in-cluster broker via an
auto-started `kubectl port-forward`; `remote` = TLS broker. Edit the catalogue to
add more. If the broker is not up, start it first (run skill).

## Publish — blocking

```sh
python3 $M pub local -t demo/x -m hello
python3 $M pub remote -t demo/x -m hello -q 1 -V mqttv5
python3 $M pub local -t demo/x -f payload.bin -r
```

## Subscribe — non-blocking, captures to file

```sh
python3 $M listen local 'demo/#' --id demo   # returns immediately
python3 $M pub local -t demo/x -m hello
python3 $M listen-log demo                   # read captured messages
python3 $M listen-status                     # LIVE/DEAD + last line
python3 $M listen-stop demo                  # or: listen-stop all
```

## Request / response — blocking, bounded (`-W 10` by default)

```sh
python3 $M rr local -t req/svc -e resp/svc -m '{"q":1}'
```

## Agentic test loop

```sh
python3 $M listen local 'test/#' --id t   # 1. listen in background
python3 $M pub local -t test/1 -m ok      # 2. drive the broker
python3 $M listen-log t                   # 3. assert -> expect: ok
python3 $M listen-stop t                  # 4. always clean up
```

## Notes

- Envs with a `tunnel` field auto-start it on `pub`/`rr`/`listen`; stop it with
  `tunnel-stop <env>`.
- Point at a different build with `MOSQ_BUILD_DIR`, a different catalogue with
  `MQTT_CLI_YAML`.
- Never put secrets in `mqtt-cli.yaml` — use `password_env: VAR`.
- Listener state/logs live under `.agents/runtime/mqtt/` (not committed).
