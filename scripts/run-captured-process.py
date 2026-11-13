#!/usr/bin/env python3
"""Run one process with typed exit/timeout metadata and redirected byte streams."""

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path


def write_json(path: Path, value) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")
    temporary.replace(path)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout-seconds", type=float, required=True)
    parser.add_argument("--stdout", required=True)
    parser.add_argument("--stderr", required=True)
    parser.add_argument("--result", required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = list(args.command)
    if command and command[0] == "--":
        command.pop(0)
    if not command:
        parser.error("a command is required after --")
    if args.timeout_seconds <= 0:
        parser.error("--timeout-seconds must be positive")

    stdout_path = Path(args.stdout)
    stderr_path = Path(args.stderr)
    result_path = Path(args.result)
    for path in (stdout_path, stderr_path, result_path):
        path.parent.mkdir(parents=True, exist_ok=True)

    started = time.perf_counter()
    result = {
        "schemaVersion": "deepdeck-captured-process/v1",
        "status": "launch_failed",
        "timedOut": False,
        "exitCode": None,
        "timeoutSeconds": args.timeout_seconds,
        "command": command,
    }
    helper_exit = 0
    try:
        with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
            process = subprocess.Popen(
                command,
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                shell=False,
                creationflags=subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0,
            )
            result["processId"] = process.pid
            try:
                result["exitCode"] = process.wait(timeout=args.timeout_seconds)
                result["status"] = "completed"
            except subprocess.TimeoutExpired:
                process.kill()
                result["exitCode"] = process.wait()
                result["timedOut"] = True
                result["status"] = "timeout"
    except OSError as error:
        result["launchError"] = f"{type(error).__name__}: {error}"
        helper_exit = 3
    finally:
        result["wallSeconds"] = round(time.perf_counter() - started, 6)
        write_json(result_path, result)

    print(json.dumps(result, ensure_ascii=False))
    return helper_exit


if __name__ == "__main__":
    raise SystemExit(main())
