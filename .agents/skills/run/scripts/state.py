#!/usr/bin/env python3
"""Read/write helper for the run skill process state file (.agents/runtime/processes.json).

All writes are atomic (write tmp + os.replace) so concurrent readers never see a
partial file. Wrapped by scripts/common.sh; not invoked directly by users.
"""
import json
import os
import sys
import time

STATE_FILE = os.environ.get("RUN_STATE_FILE")
if not STATE_FILE:
    sys.stderr.write("RUN_STATE_FILE is not set\n")
    sys.exit(2)


def load():
    try:
        with open(STATE_FILE, encoding="utf-8") as fh:
            data = json.load(fh)
        if isinstance(data, dict) and isinstance(data.get("apps"), dict):
            return data
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        pass
    return {"apps": {}}


def save(data):
    directory = os.path.dirname(STATE_FILE)
    if directory:
        os.makedirs(directory, exist_ok=True)
    tmp = f"{STATE_FILE}.{os.getpid()}.tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2, sort_keys=True)
        fh.write("\n")
    os.replace(tmp, STATE_FILE)


def cmd_set(args):
    name, pid, log_path, workdir, command, port = args
    data = load()
    data["apps"][name] = {
        "pid": int(pid),
        "startedAt": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "startedAtEpoch": int(time.time()),
        "logPath": log_path,
        "workingDirectory": workdir,
        "command": command,
        "port": int(port) if port else None,
    }
    save(data)


def cmd_remove(args):
    (name,) = args
    data = load()
    data["apps"].pop(name, None)
    save(data)


def cmd_field(args):
    name, field = args
    app = load()["apps"].get(name)
    if not app:
        return 1
    value = app.get(field)
    if value is None:
        return 1
    print(value)
    return 0


def cmd_names(_args):
    for name in sorted(load()["apps"]):
        print(name)
    return 0


def cmd_list(_args):
    for name, app in sorted(load()["apps"].items()):
        print(
            "\t".join(
                [
                    name,
                    str(app.get("pid", "")),
                    str(app.get("startedAtEpoch", "")),
                    str(app.get("port", "") or ""),
                    app.get("logPath", ""),
                    app.get("workingDirectory", ""),
                    app.get("command", ""),
                ]
            )
        )
    return 0


def cmd_uptime(args):
    (epoch,) = args
    seconds = max(0, int(time.time()) - int(epoch))
    hours, remainder = divmod(seconds, 3600)
    minutes, seconds = divmod(remainder, 60)
    print(f"{hours:02d}:{minutes:02d}:{seconds:02d}")
    return 0


def cmd_dump(_args):
    print(json.dumps(load(), indent=2, sort_keys=True))
    return 0


COMMANDS = {
    "set": cmd_set,
    "remove": cmd_remove,
    "field": cmd_field,
    "names": cmd_names,
    "list": cmd_list,
    "uptime": cmd_uptime,
    "dump": cmd_dump,
}


def main():
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        sys.stderr.write(f"usage: state.py {{{'|'.join(COMMANDS)}}} ...\n")
        return 2
    return COMMANDS[sys.argv[1]](sys.argv[2:])


if __name__ == "__main__":
    sys.exit(main())
