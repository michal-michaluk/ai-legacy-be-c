#!/usr/bin/env python3
"""run-k3s — boot a single-instance k3s and run the MQTT/OTel demo stack.

The stack (namespace `otel-mqtt-demo`) is three layers wired by name:
  mosquitto (WITH_OTEL, OTLP metrics push) <- settings-service-a / -b (MQTT 5
  notifications with W3C trace context) -> otel-collector (traces + metrics).

k3s is booted directly in a privileged container via podman/docker (no k3d), as
the run-skill paradigm prescribes for test clusters. State lives in
.agents/runtime/k3s-demo.json; the kubeconfig in .agents/runtime/k3s-demo-kubeconfig.yaml.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

CLUSTER = "mosq-otel-demo"
K3S_IMAGE = "rancher/k3s:v1.35.5-k3s1"
NAMESPACE = "otel-mqtt-demo"
MOSQUITTO_IMAGE = "mosquitto-otel:dev"
SERVICE_IMAGE = "settings-service-mqtt:dev"
COLLECTOR_IMAGE = "otel/opentelemetry-collector-contrib:0.127.0"
POSTGRES_IMAGE = "postgres:16-alpine"
DEMO_TRIGGER_TOKEN = os.environ.get("DEMO_TRIGGER_TOKEN", "demo-token-123")
SERVICE_LOCAL_PORT = 18080


def repo_root() -> Path:
    here = Path(__file__).resolve()
    for parent in [here, *here.parents]:
        if (parent / "demo" / "k3s" / "k8s").is_dir():
            return parent
    sys.exit("run-k3s: could not locate repo root (demo/k3s/k8s)")


ROOT = repo_root()
K8S_DIR = ROOT / "demo" / "k3s" / "k8s"
MOSQUITTO_DOCKERFILE = ROOT / "demo" / "k3s" / "docker" / "mosquitto-otel.Dockerfile"
MOSQUITTO_IGNORE = ROOT / "demo" / "k3s" / "docker" / "mosquitto.dockerignore"
SERVICE_DIR = ROOT / "demo" / "k3s" / "services" / "settings-service"
RUNTIME_DIR = ROOT / ".agents" / "runtime"

# Trace backends with a web UI. `ui --backend <name>` port-forwards the service.
BACKENDS = {
    "jaeger": {
        "service": "jaeger", "port": 16686, "probe": "/", "label": "Jaeger UI",
        "hint": "pick service 'settings-service-a' or '-b'; spans mqtt.publish / mqtt.receive",
    },
    "openobserve": {
        "service": "openobserve", "port": 5080, "probe": "/web/", "label": "OpenObserve UI",
        "hint": "login admin@demo.local / Complexpass#123, then menu Traces",
    },
}


def pf_pid(backend: str) -> Path:
    return RUNTIME_DIR / f"{backend}-portforward.pid"


STATE_FILE = RUNTIME_DIR / "k3s-demo.json"
KUBECONFIG = RUNTIME_DIR / "k3s-demo-kubeconfig.yaml"


def runtime() -> str:
    for rt_name in ("podman", "docker"):
        if shutil.which(rt_name):
            try:
                subprocess.run([rt_name, "info"], capture_output=True, timeout=20, check=True)
                return rt_name
            except Exception:
                continue
    sys.exit(
        "run-k3s: no working container runtime. Start it first:\n"
        "  podman machine start    # or Docker Desktop"
    )


_RT_BASE: list[str] | None = None


def _rootful_connection() -> list[str]:
    """k3s' kubelet needs real privileges (/dev/kmsg, cgroup writes). A rootless
    podman machine runs containers in a user namespace where kubelet refuses to
    start, so prefer the machine's rootful connection when present."""
    override = os.environ.get("RUN_K3S_PODMAN_CONNECTION")
    if override:
        return ["--connection", override]
    try:
        info = subprocess.run(
            ["podman", "info", "--format", "{{.Host.Security.Rootless}}"],
            capture_output=True, text=True, timeout=20,
        )
        if info.stdout.strip() == "true":
            conns = subprocess.run(
                ["podman", "system", "connection", "list", "--format", "{{.Name}}"],
                capture_output=True, text=True, timeout=20,
            )
            if "podman-machine-default-root" in conns.stdout.split():
                return ["--connection", "podman-machine-default-root"]
    except Exception:
        pass
    return []


def rt_base() -> list[str]:
    global _RT_BASE
    if _RT_BASE is None:
        r = runtime()
        _RT_BASE = [r, *_rootful_connection()] if r == "podman" else [r]
    return _RT_BASE


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, text=True, **kw)


def run_env() -> dict:
    env = dict(os.environ)
    env["KUBECONFIG"] = str(KUBECONFIG)
    return env


def kubectl(*args: str, capture: bool = True, check: bool = False) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["kubectl", *args],
        text=True,
        env=run_env(),
        capture_output=capture,
        check=check,
    )


def rt(*args: str, capture: bool = True, check: bool = False) -> subprocess.CompletedProcess:
    return subprocess.run([*rt_base(), *args], text=True, capture_output=capture, check=check)


def log(msg: str) -> None:
    print(f"[run-k3s] {msg}", flush=True)


def container_running() -> bool:
    out = rt("ps", "--format", "{{.Names}}", capture=True).stdout
    return CLUSTER in out.split()


def save_state() -> None:
    RUNTIME_DIR.mkdir(parents=True, exist_ok=True)
    state = {
        "cluster": CLUSTER,
        "runtime": runtime(),
        "kubeconfig": str(KUBECONFIG),
        "namespace": NAMESPACE,
        "images": [MOSQUITTO_IMAGE, SERVICE_IMAGE, COLLECTOR_IMAGE, POSTGRES_IMAGE],
    }
    STATE_FILE.write_text(json.dumps(state, indent=2))


def ensure_cluster() -> None:
    RUNTIME_DIR.mkdir(parents=True, exist_ok=True)
    if container_running():
        log(f"k3s container '{CLUSTER}' already running")
    else:
        log(f"booting k3s ({K3S_IMAGE}) via {runtime()}...")
        rt("rm", "-f", CLUSTER)
        rt(
            "run",
            "-d",
            "--privileged",
            "--name",
            CLUSTER,
            "--cgroupns=host",
            "-p",
            "6443:6443",
            K3S_IMAGE,
            "server",
        )
    log("waiting for kubeconfig + node Ready...")
    for _ in range(90):
        proc = rt("exec", CLUSTER, "cat", "/etc/rancher/k3s/k3s.yaml")
        if proc.returncode == 0 and "server:" in proc.stdout:
            text = proc.stdout.replace("https://127.0.0.1:6443", "https://localhost:6443")
            KUBECONFIG.write_text(text)
            break
        time.sleep(2)
    else:
        sys.exit("run-k3s: k3s did not produce a kubeconfig")
    for _ in range(90):
        proc = kubectl("get", "nodes")
        if proc.returncode == 0 and " Ready " in proc.stdout.replace("\n", " "):
            break
        time.sleep(2)
    kubectl("wait", "--for=condition=Ready", "node", "--all", "--timeout=180s")
    save_state()


def image_missing(tag: str) -> bool:
    return rt("image", "inspect", tag).returncode != 0


def build_mosquitto() -> None:
    if not image_missing(MOSQUITTO_IMAGE):
        log(f"{MOSQUITTO_IMAGE} present (rebuild with: rebuild mosquitto)")
        return
    log("building mosquitto WITH_OTEL image...")
    rt(
        "build",
        "-f",
        str(MOSQUITTO_DOCKERFILE),
        "--ignorefile",
        str(MOSQUITTO_IGNORE),
        "-t",
        MOSQUITTO_IMAGE,
        str(ROOT / "mosquitto"),
        capture=False,
        check=True,
    )


def build_service() -> None:
    if not image_missing(SERVICE_IMAGE):
        log(f"{SERVICE_IMAGE} present (rebuild with: rebuild settings-service)")
        return
    log("building settings-service-mqtt image (musl, a few minutes)...")
    rt("build", "-t", SERVICE_IMAGE, str(SERVICE_DIR), capture=False, check=True)


def import_image(tag: str) -> None:
    log(f"importing {tag} into k3s containerd...")
    rt("pull", tag)
    save = subprocess.Popen(
        [*rt_base(), "save", tag], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
    )
    imp = subprocess.run(
        [*rt_base(), "exec", "-i", CLUSTER, "ctr", "-n", "k8s.io", "images", "import", "-"],
        stdin=save.stdout,
        capture_output=True,
        text=True,
    )
    save.wait()
    if imp.returncode != 0:
        sys.exit(f"run-k3s: failed to import {tag}: {imp.stderr}")


def apply_stack() -> None:
    log("applying manifests (kustomize)...")
    kubectl("-n", NAMESPACE, "delete", "job", "db-migrate", "--ignore-not-found")
    out = kubectl("apply", "-k", str(K8S_DIR))
    if out.returncode != 0:
        sys.exit(f"run-k3s: apply failed:\n{out.stderr}")
    for dep in ("otel-collector", "mosquitto", "postgres", "settings-service-a", "settings-service-b"):
        kubectl("-n", NAMESPACE, "rollout", "status", f"deploy/{dep}", "--timeout=240s")
    kubectl("-n", NAMESPACE, "wait", "--for=condition=complete", "job/db-migrate", "--timeout=240s")
    log("stack deployed")


def cmd_deploy(args: argparse.Namespace) -> None:
    ensure_cluster()
    if args.skip_build and (image_missing(MOSQUITTO_IMAGE) or image_missing(SERVICE_IMAGE)):
        sys.exit("run-k3s: --skip-build set but images are missing")
    if not args.skip_build:
        build_mosquitto()
        build_service()
    for tag in (MOSQUITTO_IMAGE, SERVICE_IMAGE, COLLECTOR_IMAGE, POSTGRES_IMAGE):
        if tag == POSTGRES_IMAGE and not image_missing(tag):
            import_image(tag)
        elif tag != POSTGRES_IMAGE:
            import_image(tag)
    apply_stack()


def pod_of(app: str) -> str | None:
    out = kubectl(
        "-n", NAMESPACE, "get", "pods", "-l", f"app={app}",
        "-o", "jsonpath={.items[0].metadata.name}",
    )
    name = out.stdout.strip()
    return name or None


def cmd_status(_: argparse.Namespace) -> None:
    if not container_running():
        print("k3s: NOT running (run: deploy)")
        return
    print(f"cluster container: {CLUSTER} ({runtime()})")
    print("--- nodes ---")
    print(kubectl("get", "nodes").stdout.strip())
    print("--- pods ---")
    print(kubectl("-n", NAMESPACE, "get", "pods", "-o", "wide").stdout.strip())
    print("--- services ---")
    print(kubectl("-n", NAMESPACE, "get", "svc").stdout.strip())
    print("--- activity ---")
    for app in ("settings-service-a", "settings-service-b"):
        logs = kubectl("-n", NAMESPACE, "logs", f"deploy/{app}", "--tail=200").stdout
        pub = logs.count("notification published")
        recv = logs.count("notification received")
        print(f"{app}: published={pub} received={recv}")
    mosq = kubectl("-n", NAMESPACE, "logs", "deploy/mosquitto", "--tail=2000").stdout
    print("mosquitto OTel:", "enabled" if "OpenTelemetry metrics export enabled" in mosq else "not seen")


def cmd_logs(args: argparse.Namespace) -> None:
    targets = {
        "mosquitto": "deploy/mosquitto",
        "collector": "deploy/otel-collector",
        "jaeger": "deploy/jaeger",
        "openobserve": "deploy/openobserve",
        "a": "deploy/settings-service-a",
        "b": "deploy/settings-service-b",
        "postgres": "deploy/postgres",
    }
    args_target = args.target or "collector"
    tgt = targets.get(args_target, args_target)
    cmd = ["-n", NAMESPACE, "logs", tgt, f"--tail={args.tail}"]
    if args.follow:
        cmd.append("-f")
    subprocess.run(["kubectl", *cmd], env=run_env())


def cmd_rebuild(args: argparse.Namespace) -> None:
    if not container_running():
        sys.exit("run-k3s: cluster not running")
    which = args.service
    if which in ("mosquitto", "all"):
        if not image_missing(MOSQUITTO_IMAGE):
            rt("rmi", "-f", MOSQUITTO_IMAGE)
        build_mosquitto()
        import_image(MOSQUITTO_IMAGE)
        kubectl("-n", NAMESPACE, "rollout", "restart", "deploy/mosquitto")
        kubectl("-n", NAMESPACE, "rollout", "status", "deploy/mosquitto", "--timeout=240s")
    if which in ("settings-service", "all"):
        if not image_missing(SERVICE_IMAGE):
            rt("rmi", "-f", SERVICE_IMAGE)
        build_service()
        import_image(SERVICE_IMAGE)
        for app in ("settings-service-a", "settings-service-b"):
            kubectl("-n", NAMESPACE, "rollout", "restart", f"deploy/{app}")
            kubectl("-n", NAMESPACE, "rollout", "status", f"deploy/{app}", "--timeout=240s")
    log(f"rebuild {which} done")


def cmd_restart(args: argparse.Namespace) -> None:
    cmd_stop(args)
    cmd_deploy(argparse.Namespace(skip_build=True))


def cmd_stop(_: argparse.Namespace) -> None:
    if container_running():
        log(f"removing k3s container '{CLUSTER}'...")
        rt("rm", "-f", CLUSTER)
    KUBECONFIG.unlink(missing_ok=True)
    log("stopped")


def cmd_publish(args: argparse.Namespace) -> None:
    pod = pod_of("mosquitto")
    if not pod:
        sys.exit("run-k3s: mosquitto pod not found")
    kubectl(
        "-n", NAMESPACE, "exec", pod, "--", "mosquitto_pub",
        "-h", "localhost", "-t", args.topic, "-m", args.message,
        capture=False,
    )


def cmd_trigger(args: argparse.Namespace) -> None:
    """Curl the demo REST trigger of one instance: it changes a config key, which
    the service publishes over MQTT to the other instance (one trace)."""
    import urllib.request

    svc = f"settings-service-{args.service}"
    pid_file = RUNTIME_DIR / f"{svc}-portforward.pid"
    alive = False
    if pid_file.exists():
        try:
            os.kill(int(pid_file.read_text().strip()), 0)
            alive = True
        except (ValueError, OSError):
            alive = False
    if not alive:
        RUNTIME_DIR.mkdir(parents=True, exist_ok=True)
        proc = subprocess.Popen(
            ["kubectl", "-n", NAMESPACE, "port-forward", f"svc/{svc}",
             f"{SERVICE_LOCAL_PORT}:80"],
            env=run_env(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        pid_file.write_text(str(proc.pid))
        for _ in range(40):
            try:
                urllib.request.urlopen(
                    f"http://localhost:{SERVICE_LOCAL_PORT}/healthz", timeout=2
                )
                break
            except Exception:
                time.sleep(0.5)

    value = json.loads(args.value)
    body = json.dumps({"value": value}).encode()
    req = urllib.request.Request(
        f"http://localhost:{SERVICE_LOCAL_PORT}/demo/config/{args.key}",
        data=body,
        method="POST",
        headers={"content-type": "application/json", "x-demo-token": DEMO_TRIGGER_TOKEN},
    )
    with urllib.request.urlopen(req) as resp:
        print(f"HTTP {resp.status}: {resp.read().decode()}")
    print(f"published via {svc}; check the other instance's logs / the trace UI")


def _pf_alive(backend: str) -> bool:
    pid_file = pf_pid(backend)
    if not pid_file.exists():
        return False
    try:
        os.kill(int(pid_file.read_text().strip()), 0)
        return True
    except (ValueError, OSError):
        return False


def start_port_forward(backend: str) -> None:
    """Port-forward a trace backend's UI to localhost for the browser."""
    import urllib.request

    if _pf_alive(backend):
        return
    cfg = BACKENDS[backend]
    RUNTIME_DIR.mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen(
        ["kubectl", "-n", NAMESPACE, "port-forward", f"svc/{cfg['service']}",
         f"{cfg['port']}:{cfg['port']}"],
        env=run_env(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    pf_pid(backend).write_text(str(proc.pid))
    for _ in range(40):
        try:
            urllib.request.urlopen(f"http://localhost:{cfg['port']}{cfg['probe']}", timeout=2)
            return
        except Exception:
            time.sleep(0.5)
    sys.exit(f"run-k3s: {backend} port-forward did not become ready")


def cmd_ui(args: argparse.Namespace) -> None:
    if args.stop:
        stopped = []
        for name in ([args.backend] if args.backend else BACKENDS):
            if _pf_alive(name):
                os.kill(int(pf_pid(name).read_text().strip()), 15)
                pf_pid(name).unlink(missing_ok=True)
                stopped.append(name)
        print(f"stopped port-forward(s): {', '.join(stopped) or 'none'}")
        return
    name = args.backend or "jaeger"
    cfg = BACKENDS[name]
    start_port_forward(name)
    print(f"{cfg['label']}: http://localhost:{cfg['port']}")
    print(cfg["hint"])
    print("stop the forward with: ui --backend %s --stop" % name)


def cmd_traces(args: argparse.Namespace) -> None:
    """Query the Jaeger HTTP API and print recent traces with their spans."""
    import urllib.request

    start_port_forward("jaeger")
    base = f"http://localhost:{BACKENDS['jaeger']['port']}"
    services = json.load(urllib.request.urlopen(f"{base}/api/services")).get("data", [])
    print("services:", ", ".join(services) or "(none yet)")
    for svc in services:
        data = json.load(
            urllib.request.urlopen(f"{base}/api/traces?service={svc}&limit={args.limit}")
        ).get("data", [])
        for trace in data:
            names = sorted({sp.get("operationName") for sp in trace.get("spans", [])})
            print(f"  {trace.get('traceID', '')[:16]}  {svc}: {names}")


TRACE_ID_RE = re.compile(r"Trace ID\s*:\s*([0-9a-f]{32})")
SPAN_NAME_RE = re.compile(r"^\s*(?:->\s*)?Name\s*:\s*(\S+)\s*$")
METRIC_NAME_RE = re.compile(r"^\s*(?:->\s*)?Name\s*:\s*(mosquitto_[a-z0-9_]+)\s*$", re.M)


def cmd_verify(_: argparse.Namespace) -> None:
    """Deterministic checks: OTel metrics pushed, and spans propagated across
    the two services over MQTT (one trace id carrying both publish + receive)."""
    if not container_running():
        sys.exit("run-k3s: cluster not running")
    failures: list[str] = []

    mosq = kubectl("-n", NAMESPACE, "logs", "deploy/mosquitto", "--tail=2000").stdout
    if "OpenTelemetry support available" in mosq:
        print("[ok] mosquitto built WITH_OTEL")
    else:
        failures.append("mosquitto does not report OpenTelemetry support")

    collector = kubectl("-n", NAMESPACE, "logs", "deploy/otel-collector", "--tail=20000").stdout

    metrics = set(METRIC_NAME_RE.findall(collector))
    if metrics:
        print(f"[ok] collector received {len(metrics)} mosquitto_* metrics (OTLP push)")
    else:
        failures.append("collector received no mosquitto_* metrics")

    trace_spans: dict[str, set[str]] = {}
    current = None
    for line in collector.splitlines():
        m = TRACE_ID_RE.search(line)
        if m:
            current = m.group(1)
            trace_spans.setdefault(current, set())
            continue
        m = SPAN_NAME_RE.match(line)
        if m and current:
            trace_spans[current].add(m.group(1))

    propagated = [
        tid
        for tid, names in trace_spans.items()
        if "mqtt.publish" in names and "mqtt.receive" in names
    ]
    if propagated:
        print(
            f"[ok] span propagation: {len(propagated)} trace(s) carry both "
            f"mqtt.publish and mqtt.receive, e.g. {propagated[0]}"
        )
    else:
        failures.append("no single trace carried both mqtt.publish and mqtt.receive")

    for app in ("settings-service-a", "settings-service-b"):
        logs = kubectl("-n", NAMESPACE, "logs", f"deploy/{app}", "--tail=300").stdout
        if "notification received" in logs and "traceparent" in logs:
            print(f"[ok] {app} received a notification with a traceparent")
        else:
            failures.append(f"{app} did not log a received notification with traceparent")

    if failures:
        print("\nVERIFY FAILED:")
        for f in failures:
            print(f"  - {f}")
        sys.exit(1)
    print("\nVERIFY PASSED")


def main() -> None:
    parser = argparse.ArgumentParser(prog="run-k3s", description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("deploy", help="create k3s, build+import images, apply the stack")
    p.add_argument("--skip-build", action="store_true", help="use existing images")
    p.set_defaults(func=cmd_deploy)

    sub.add_parser("status", help="cluster, pods, services, activity").set_defaults(func=cmd_status)

    p = sub.add_parser("logs", help="stream logs")
    p.add_argument("target", nargs="?", help="mosquitto|collector|a|b|postgres|<pod>")
    p.add_argument("--follow", "-f", action="store_true")
    p.add_argument("--tail", type=int, default=200)
    p.set_defaults(func=cmd_logs)

    p = sub.add_parser("rebuild", help="rebuild+import+rollout one service")
    p.add_argument("service", choices=["mosquitto", "settings-service", "all"])
    p.set_defaults(func=cmd_rebuild)

    sub.add_parser("restart", help="stop + deploy (images kept)").set_defaults(func=cmd_restart)
    sub.add_parser("stop", help="delete the k3s cluster").set_defaults(func=cmd_stop)
    sub.add_parser("verify", help="prove metrics push + span propagation").set_defaults(func=cmd_verify)

    p = sub.add_parser("publish", help="publish a message via the mosquitto pod")
    p.add_argument("topic")
    p.add_argument("message")
    p.set_defaults(func=cmd_publish)

    p = sub.add_parser("trigger", help="curl the demo REST trigger of one instance")
    p.add_argument("key", help="config key, e.g. theme")
    p.add_argument("value", help='JSON value, e.g. \'{"color":"blue"}\'')
    p.add_argument("--service", choices=["a", "b"], default="a")
    p.set_defaults(func=cmd_trigger)

    p = sub.add_parser("ui", help="open a trace backend's web UI (port-forward)")
    p.add_argument("--backend", choices=sorted(BACKENDS), help="jaeger | openobserve")
    p.add_argument("--stop", action="store_true", help="stop the port-forward")
    p.set_defaults(func=cmd_ui)

    p = sub.add_parser("traces", help="list recent traces from the Jaeger API")
    p.add_argument("--limit", type=int, default=5)
    p.set_defaults(func=cmd_traces)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
