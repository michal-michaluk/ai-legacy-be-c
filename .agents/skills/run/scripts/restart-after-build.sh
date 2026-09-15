#!/bin/sh
# Build then restart one app, or all apps.
#
#   restart-after-build.sh broker | dashboard | all
#
# Build logs go to .agents/runtime/logs/build/<app>-<step>.<ts>.log and the
# broker build fails fast on a non-zero exit code.
set -u

. "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)/common.sh"

if [ "$#" -ne 1 ]; then
    printf 'usage: %s <broker|dashboard|all>\n' "$0" >&2
    exit 2
fi
target="$1"

build_log() { # app step
    mkdir -p "$LOG_DIR/build"
    printf '%s/build/%s-%s.%s.log\n' "$LOG_DIR" "$1" "$2" "$(date +%Y%m%d-%H%M%S)"
}

build_broker() {
    log=$(build_log broker cmake)
    printf 'Building broker: cmake --build %s\n' "$BUILD_DIR"
    if [ ! -f "$BUILD_DIR/CMakeCache.txt" ]; then
        printf 'error: no cmake build dir at %s\n' "$BUILD_DIR" >&2
        printf 'configure it: cmake -S %s -B %s -G Ninja -DCMAKE_BUILD_TYPE=Release\n' "$MOSQ_ROOT" "$BUILD_DIR" >&2
        return 1
    fi
    if cmake --build "$BUILD_DIR" -j >"$log" 2>&1; then
        printf 'broker build OK (%s), last lines:\n' "$log"
    else
        printf 'broker build FAILED (exit=%s). Last 20 lines of %s:\n' "$?" "$log" >&2
        tail -n 20 "$log" >&2
        return 1
    fi
    tail -n 5 "$log"
}

restart_one() { # app
    case "$1" in
        broker)
            build_broker || return 1
            "$SCRIPT_DIR/stop-process.sh" broker
            "$SCRIPT_DIR/start-broker.sh"
            ;;
        dashboard)
            printf 'dashboard is static; Tailwind CSS is prebuilt at dashboard/src/tailwind/styles.css — no build step.\n'
            "$SCRIPT_DIR/stop-process.sh" dashboard
            "$SCRIPT_DIR/start-dashboard.sh"
            ;;
        *)
            printf "unknown app '%s'\n" "$1" >&2
            return 2
            ;;
    esac
}

case "$target" in
    all)
        build_broker || exit 1
        "$SCRIPT_DIR/stop-all.sh"
        "$SCRIPT_DIR/start-all.sh"
        ;;
    broker|dashboard)
        restart_one "$target"
        ;;
    *)
        printf "usage: %s <broker|dashboard|all>\n" "$0" >&2
        exit 2
        ;;
esac
