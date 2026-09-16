// demo/record.js — chaptered demo of the Kubernetes-native OTel observability.
//
// Backend/CLI feature: there is no web UI. The recorded surface is a real bash
// terminal served by ttyd (demo/scripts/start-terminal.sh) and driven through
// Playwright; the screencast is the demo-skill's Playwright screencast API, and
// mark() shows a chapter card and records the chapter timestamp.
//
// Run (from the repo root):
//   demo/scripts/start-terminal.sh
//   playwright-cli resize 1280 720
//   playwright-cli open http://127.0.0.1:7699
//   playwright-cli --raw run-code --filename=demo/record.js > demo/chapters.json
//   demo/scripts/stop-terminal.sh
//
// record.js starts and stops the broker and collector itself; the setup commands
// run before the screencast starts, so the first chapter (Intro) is t = 0.
async page => {
  const OUT = "demo/kubernetes-native.webm";
  const SIZE = { width: 1280, height: 720 };

  const t0 = { v: 0 };
  const chapters = [];

  async function mark(title, description) {
    chapters.push({ title, t: +((Date.now() - t0.v) / 1000).toFixed(2) });
    await page.screencast.showChapter(title, { description, duration: 2400 });
  }

  async function run(cmd, wait = 1200, delay = 5) {
    await page.keyboard.type(cmd, { delay });
    await page.keyboard.press("Enter");
    await page.waitForTimeout(wait);
  }

  await page.locator(".xterm").click();
  await page.waitForTimeout(300);
  await page.keyboard.press("Control+C");
  await page.waitForTimeout(300);

  // Pre-roll: reset scratch state and get a clean screen, not recorded.
  await run("demo/scripts/demo-setup.sh", 1000);
  await run("clear", 600, 0);

  t0.v = Date.now();
  await page.screencast.start({ path: OUT, size: SIZE });

  try {
    await mark("Intro", "Kubernetes-native Mosquitto observability — C2 OTLP metrics · C3 trace carry · C4 opt-in flag");
    await run("ls docs/specs/kubernetes-native/spec.md mosquitto/test/broker/24-otel-otlp-metrics.py mosquitto/test/broker/25-trace-context-happy.py mosquitto/test/otel/zero-overhead.sh", 2200);

    await mark("1. Opt-in flag reports the OpenTelemetry capability", "OFF build: 'support NOT available'; ON build: 'support available' + export enabled");
    await run("demo/scripts/start-broker.sh off", 2000);
    await run("demo/scripts/stop-broker.sh", 1000);
    await run("demo/scripts/start-broker.sh on http://127.0.0.1:43183", 2200);
    await run("demo/scripts/stop-broker.sh", 1000);

    await mark("2. OTLP metrics reach the collector", "collector JSONL has mosquitto_* metrics (Sum, CUMULATIVE, monotonic) with service.name=mosquitto");
    await run("demo/scripts/start-collector.sh", 3000);
    await run("demo/scripts/start-broker.sh on http://127.0.0.1:43183", 2000);
    await run("demo/scripts/pub-traffic.sh", 3500);
    await run("demo/scripts/metrics-summary.sh", 4500);

    await mark("3. Trace context carried PUB -> SUB byte-identical", "MQTT 5 subscriber prints the same traceparent/tracestate the publisher sent");
    await run("M=$PWD/.agents/tmp/build-otel-on/client", 600);
    await run("$M/mosquitto_sub -V mqttv5 -p 18833 -t 'demo/#' -C 1 -F '%P | %p' > .agents/tmp/demo-sub5.txt 2>&1 &", 1000);
    await run("$M/mosquitto_pub -V mqttv5 -p 18833 -t demo/trace -m 'order-123' -D publish user-property traceparent 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01 -D publish user-property tracestate vendor1=value1,vendor2=value2", 1500);
    await run("cat .agents/tmp/demo-sub5.txt", 3000);

    await mark("4. MQTT 3.1.1 has no metadata channel", "MQTT 3.1.1 subscriber prints no user properties");
    await run("$M/mosquitto_sub -V mqttv311 -p 18833 -t 'demo/#' -C 1 -F '[%P] | %p' > .agents/tmp/demo-sub311.txt 2>&1 &", 1000);
    await run("$M/mosquitto_pub -V mqttv5 -p 18833 -t demo/trace -m 'order-456' -D publish user-property traceparent 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01", 1500);
    await run("cat .agents/tmp/demo-sub311.txt", 2600);

    await mark("5. Zero overhead when the flag is off", "OFF binary: no otel/curl symbols, no libcurl; ON binary: both; only libcurl added");
    await run("demo/scripts/stop-all.sh", 1200);
    await run("mosquitto/test/otel/zero-overhead.sh .agents/tmp/build-otel-off .agents/tmp/build-otel-on", 21000);

    await mark("Outro — C2 · C3 · C4 proven end-to-end", "metrics exported over OTLP/HTTP JSON · trace context carried unchanged · opt-in flag contained");
    await run("demo/scripts/stop-all.sh", 1200);
  } finally {
    await page.screencast.stop();
  }

  return {
    title: "Kubernetes-native Mosquitto observability",
    durationSec: +((Date.now() - t0.v) / 1000).toFixed(2),
    chapters,
  };
}
