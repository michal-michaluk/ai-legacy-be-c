#!/usr/bin/env python3
"""mqtt-cli: thin, env-aware wrapper over mosquitto_pub / mosquitto_sub / mosquitto_rr.

Profiles come from .agents/mqtt-cli.yaml. Guide: .agents/skills/mqtt-cli/SKILL.md

  envs | show <env>
  pub  <env> [pub args...]        blocking
  rr   <env> [rr args...]         blocking, bounded by -W 10 unless given
  listen <env> <topic...> [--id ID] [--file F] [-- extra sub args]
  listen-status | listen-log <ID> | listen-stop <ID|all>
  tunnel-start <env> | tunnel-stop <env>
"""
import json
import os
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

SCRIPT = Path(__file__).resolve()
AGENTS = SCRIPT.parents[3]
ROOT = AGENTS.parent
CONF = Path(os.environ.get("MQTT_CLI_YAML", AGENTS / "mqtt-cli.yaml"))
RUNTIME = AGENTS / "runtime" / "mqtt"
STATE = RUNTIME / "state.json"
BUILD = Path(os.environ.get("MOSQ_BUILD_DIR", ROOT / "mosquitto" / "build"))


def die(msg, code=2):
    print(f"mqtt-cli: {msg}", file=sys.stderr)
    sys.exit(code)


def load_conf():
    if not CONF.is_file():
        die(f"catalogue not found: {CONF}")
    import yaml

    doc = yaml.safe_load(CONF.read_text()) or {}
    envs = doc.get("environments") or {}
    if not envs:
        die(f"no environments in {CONF}")
    return envs, doc.get("default")


def resolve(name):
    envs, default = load_conf()
    name = default if name in (None, "", "@default") else name
    if name not in envs:
        die(f"unknown env '{name}' (have: {', '.join(envs)})")
    return name, envs[name] or {}


def client_bin(name):
    for p in [BUILD / "client" / name, *sorted(BUILD.glob(f"*/client/{name}"))]:
        if p.is_file():
            return str(p)
    die(f"{name} not built under {BUILD}; build first (see the run skill)")


def _var(var):
    return os.environ.get(var) if var else None


def conn_flags(env):
    if env.get("unix"):
        return ["--unix", env["unix"]]
    f = []
    if env.get("host"):
        f += ["-h", str(env["host"])]
    if env.get("port"):
        f += ["-p", str(env["port"])]
    if env.get("ws"):
        f += ["--ws"]
    if env.get("mqtt_version"):
        f += ["-V", str(env["mqtt_version"])]
    user = env.get("username") or _var(env.get("username_env"))
    pw = env.get("password") or _var(env.get("password_env"))
    if user:
        f += ["-u", user]
    if pw:
        f += ["-P", pw]
    tls = env.get("tls") or {}
    for key, flag in (("cafile", "--cafile"), ("capath", "--capath"),
                      ("cert", "--cert"), ("key", "--key")):
        if tls.get(key):
            f += [flag, tls[key]]
    if tls.get("insecure"):
        f += ["--insecure"]
    return f + list(env.get("extra_args") or [])


def _state():
    if STATE.is_file():
        try:
            return json.loads(STATE.read_text())
        except ValueError:
            pass
    return {"listeners": {}, "tunnels": {}}


def _save(st):
    RUNTIME.mkdir(parents=True, exist_ok=True)
    tmp = STATE.with_suffix(".tmp")
    tmp.write_text(json.dumps(st, indent=2))
    os.replace(tmp, STATE)


def _alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def _kill(pid):
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(pid, sig)
        except OSError:
            try:
                os.kill(pid, sig)
            except OSError:
                return
        for _ in range(50):
            if not _alive(pid):
                return
            time.sleep(0.1)


def _uptime(epoch):
    if not epoch:
        return "?"
    s = max(0, int(time.time() - epoch))
    return f"{s // 3600:02d}:{s % 3600 // 60:02d}:{s % 60:02d}"


def _wait_port(host, port, timeout=10):
    end = time.time() + timeout
    while time.time() < end:
        try:
            with socket.create_connection((host, int(port)), 0.3):
                return True
        except OSError:
            time.sleep(0.2)
    return False


def ensure_tunnel(name, env):
    cmd = env.get("tunnel")
    if not cmd:
        return
    st = _state()
    t = st["tunnels"].get(name)
    if t and _alive(t["pid"]):
        return
    log = RUNTIME / "logs" / f"tunnel-{name}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    fh = open(log, "w")
    p = subprocess.Popen(cmd, shell=True, stdout=fh, stderr=subprocess.STDOUT,
                         start_new_session=True)
    st["tunnels"][name] = {"pid": p.pid, "startedAtEpoch": int(time.time()),
                           "command": cmd, "log": str(log)}
    _save(st)
    host, port = env.get("host", "127.0.0.1"), env.get("port")
    if port and not _wait_port(host, port):
        print(f"mqtt-cli: warning: tunnel '{name}' not ready; see {log}", file=sys.stderr)


def cmd_envs(_):
    envs, default = load_conf()
    for n, e in envs.items():
        e = e or {}
        where = e.get("unix") or f"{e.get('host', '?')}:{e.get('port', '?')}"
        mark = "*" if n == default else " "
        print(f"{mark} {n:10} {where}")


def cmd_show(argv):
    name, env = resolve(argv[0] if argv else None)
    fields = {
        "host": env.get("host"), "port": env.get("port"), "unix": env.get("unix"),
        "ws": env.get("ws"), "mqtt_version": env.get("mqtt_version"),
        "username": env.get("username") or (_var(env.get("username_env")) and f"${{{env['username_env']}}}"),
        "password": "***" if (env.get("password") or env.get("password_env")) else None,
        "tls": env.get("tls"), "tunnel": env.get("tunnel"),
    }
    print(f"env: {name}")
    for k, v in fields.items():
        if v not in (None, False):
            print(f"  {k}: {v}")
    print("  flags: " + " ".join(conn_flags(env)))


def _split(argv):
    if not argv:
        die("missing <env> (see: mqtt.py envs)")
    return argv[0], argv[1:]


def cmd_passthrough(binary, argv, default_w=None):
    name, args = _split(argv)
    name, env = resolve(name)
    ensure_tunnel(name, env)
    if default_w and "-W" not in args:
        args = ["-W", str(default_w)] + args
    sys.exit(subprocess.call([client_bin(binary)] + conn_flags(env) + args))


def cmd_listen(argv):
    name, rest = _split(argv)
    topics, ident, out, extra = [], None, None, []
    i = 0
    while i < len(rest):
        a = rest[i]
        if a == "--id":
            ident = rest[i + 1]
            i += 2
        elif a == "--file":
            out = rest[i + 1]
            i += 2
        elif a == "--":
            extra = rest[i + 1:]
            break
        else:
            topics.append(a)
            i += 1
    if not topics:
        die("listen needs at least one topic")
    name, env = resolve(name)
    ensure_tunnel(name, env)
    ident = ident or f"sub-{int(time.time())}"
    path = Path(out) if out else RUNTIME / "logs" / f"{ident}.{time.strftime('%Y%m%d-%H%M%S')}.txt"
    path.parent.mkdir(parents=True, exist_ok=True)
    cmd = [client_bin("mosquitto_sub")] + conn_flags(env) + extra
    for t in topics:
        cmd += ["-t", t]
    fh = open(path, "w")
    p = subprocess.Popen(cmd, stdout=fh, stderr=subprocess.STDOUT, start_new_session=True)
    st = _state()
    st["listeners"][ident] = {"pid": p.pid, "env": name, "topics": topics,
                              "file": str(path), "startedAtEpoch": int(time.time())}
    _save(st)
    time.sleep(0.3)
    print(f"{'LIVE' if _alive(p.pid) else 'DIED'} listener {ident} pid={p.pid} env={name} file={path}")
    if not _alive(p.pid):
        print(path.read_text(errors="replace"))
    print(f"  log:  {SCRIPT} listen-log {ident}")
    print(f"  stop: {SCRIPT} listen-stop {ident}")


def cmd_status(_):
    ls = _state().get("listeners", {})
    if not ls:
        print("no listeners")
        return
    for ident, r in ls.items():
        tail = ""
        p = Path(r["file"])
        if p.is_file():
            lines = p.read_text(errors="replace").splitlines()
            if lines:
                tail = lines[-1][:100]
        print(f"{'LIVE' if _alive(r['pid']) else 'DEAD'} {ident:16} pid={r['pid']} "
              f"env={r['env']} up={_uptime(r.get('startedAtEpoch'))} file={r['file']}")
        if tail:
            print(f"     last: {tail}")


def cmd_log(argv):
    if not argv:
        die("usage: mqtt.py listen-log <id>")
    r = _state()["listeners"].get(argv[0])
    if not r:
        die(f"no such listener: {argv[0]}")
    p = Path(r["file"])
    sys.stdout.write(p.read_text(errors="replace") if p.is_file() else "")


def cmd_stop(argv):
    if not argv:
        die("usage: mqtt.py listen-stop <id|all>")
    st = _state()
    targets = list(st["listeners"]) if argv[0] == "all" else [argv[0]]
    for ident in targets:
        r = st["listeners"].pop(ident, None)
        if not r:
            print(f"unknown listener: {ident}")
            continue
        _kill(r["pid"])
        print(f"stopped {ident}")
    _save(st)


def cmd_tunnel(argv):
    if not argv:
        die("usage: mqtt.py tunnel-start|tunnel-stop <env>")
    action = argv[0]
    name, env = resolve(argv[1] if len(argv) > 1 else None)
    st = _state()
    if action == "tunnel-start":
        ensure_tunnel(name, env)
        print(f"tunnel {name} up")
    elif action == "tunnel-stop":
        t = st["tunnels"].pop(name, None)
        _save(st)
        if t:
            _kill(t["pid"])
            print(f"tunnel {name} stopped")
        else:
            print(f"no tunnel for {name}")
    else:
        die(f"unknown command: {action}")


COMMANDS = {
    "envs": cmd_envs,
    "show": cmd_show,
    "listen": cmd_listen,
    "listen-status": cmd_status,
    "listen-log": cmd_log,
    "listen-stop": cmd_stop,
    "tunnel-start": cmd_tunnel,
    "tunnel-stop": cmd_tunnel,
}


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__.strip())
        return 0
    cmd, rest = argv[0], argv[1:]
    if cmd == "pub":
        cmd_passthrough("mosquitto_pub", rest)
    elif cmd == "rr":
        cmd_passthrough("mosquitto_rr", rest, default_w=10)
    elif cmd in COMMANDS:
        COMMANDS[cmd](rest)
    else:
        die(f"unknown command: {cmd} (see --help)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
