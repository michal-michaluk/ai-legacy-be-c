# Demo — Kubernetes-native Mosquitto observability

Chaptered demo of capabilities **C2** (OTLP metrics), **C3** (trace-context carry) and
**C4** (opt-in `WITH_OTEL` flag). Backend/CLI feature — there is no web UI — so the demo
runs the real terminal workflow: broker (`WITH_OTEL=ON/OFF`) + local
`otel/opentelemetry-collector-contrib` + the MQTT client CLIs, recorded from a real bash
terminal served by `ttyd` with the `demo` skill's Playwright screencast.

## Artefacts

| File | What |
| --- | --- |
| `scenarios.md` | the 5 acceptance scenarios (one chapter each) |
| `record.js` | Playwright driver — records the video and emits the chapter times |
| `chapters.json` | `{ title, durationSec, chapters:[{ title, t }] }`, written by `record.js` |
| `kubernetes-native.webm` | raw screencast |
| `kubernetes-native.chapters.mkv` | **final** — chapters embedded (mpv/VLC chapter navigation) |
| `scripts/` | terminal + broker/collector helpers used by the driver |

## Reproduce

Prerequisites: built brokers at `.agents/tmp/build-otel-{off,on}`, the collector binary at
`.agents/tmp/otelcol/otelcol-contrib`, `ttyd`, `node`, `ffmpeg`, and `playwright-cli`.

```sh
demo/scripts/start-terminal.sh                     # ttyd on 127.0.0.1:7699
playwright-cli resize 1280 720
playwright-cli open http://127.0.0.1:7699
playwright-cli --raw run-code --filename=demo/record.js > demo/chapters.json
demo/scripts/stop-terminal.sh

node ~/.agents/skills/demo/references/embed-chapters.mjs \
  demo/kubernetes-native.webm demo/chapters.json demo/kubernetes-native.chapters.mkv
```

`record.js` starts and stops the broker and collector itself; its setup commands run
before the screencast starts, so the first chapter (`Intro`) is at `t = 0`.

## Share for review

```sh
node demo/scripts/serve-demo.mjs 8099                 # static server with Range support
cloudflared tunnel --no-autoupdate --url http://127.0.0.1:8099
```

The `https://<random>.trycloudflare.com/kubernetes-native.webm` URL plays the video in a
browser (Range-capable, so seeking works). The `.mkv` (chapter navigation) is a local
player artefact — browsers cannot play Matroska.
