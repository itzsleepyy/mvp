#!/usr/bin/env python3
"""Deterministic same-terminal Codex runner smoke test.

Run after `cargo build`: `python3 tests/codex_runner_e2e.py`.
"""

import fcntl
import os
import pathlib
import pty
import select
import struct
import subprocess
import tempfile
import termios
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = ROOT / "target" / "debug" / "waitstate"


def wait_for(fd, captured, needle, timeout=8):
    end = time.time() + timeout
    target = needle.encode()
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], 0.1)
        if ready:
            try:
                captured.extend(os.read(fd, 65536))
            except OSError:
                pass
        if target in captured:
            return True
    return False


def main():
    if not BIN.exists():
        raise SystemExit("run `cargo build` first")

    with tempfile.TemporaryDirectory(prefix="waitstate-runner-") as temporary:
        temp = pathlib.Path(temporary)
        fake_bin = temp / "bin"
        codex_home = temp / "codex"
        fake_bin.mkdir()
        codex_home.mkdir()
        fake_codex = fake_bin / "codex"
        fake_codex.write_text(
            "#!/bin/sh\n"
            "printf 'FAKE CODEX READY\\r\\n'\n"
            "sleep 1\n"
            "IFS= read -r prompt\n"
            f"'{BIN}' hook codex working >/dev/null\n"
            "printf 'HIDDEN CHILD OUTPUT: %s\\r\\n' \"$prompt\"\n"
            "sleep 1\n"
            f"'{BIN}' hook codex completed >/dev/null\n"
            "printf 'FAKE CODEX RESTORED\\r\\n'\n"
            "sleep 1\n"
        )
        fake_codex.chmod(0o755)

        env = dict(os.environ)
        env["HOME"] = str(temp)
        env["CODEX_HOME"] = str(codex_home)
        env["PATH"] = str(fake_bin) + os.pathsep + env["PATH"]
        subprocess.run(
            [BIN, "codex", "install"],
            cwd=ROOT,
            env=env,
            check=True,
            capture_output=True,
        )

        pid, fd = pty.fork()
        if pid == 0:
            os.chdir(ROOT)
            os.execve(str(BIN), [str(BIN), "codex", "run"], env)
            os._exit(1)

        captured = bytearray()
        try:
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
            assert wait_for(fd, captured, "FAKE CODEX READY")
            # The fake delays for longer than the runner's 50 ms input poll,
            # guarding against treating an ordinary poll timeout as EOF.
            time.sleep(1.2)
            os.write(fd, b"test prompt\n")
            assert wait_for(fd, captured, "STACK JUMP")
            assert wait_for(fd, captured, "FAKE CODEX RESTORED")
            assert b"HIDDEN CHILD OUTPUT" in captured

            end = time.time() + 15
            status = None
            while time.time() < end:
                ready, _, _ = select.select([fd], [], [], 0.1)
                if ready:
                    try:
                        captured.extend(os.read(fd, 65536))
                    except OSError:
                        pass
                done, value = os.waitpid(pid, os.WNOHANG)
                if done:
                    status = value
                    break
            assert status is not None, "managed runner did not exit with its child"
            assert os.waitstatus_to_exitcode(status) == 0
        finally:
            try:
                os.kill(pid, 9)
                os.waitpid(pid, 0)
            except (ProcessLookupError, ChildProcessError):
                pass
            os.close(fd)

    print("SAME-TERMINAL CODEX RUNNER E2E PASSED")


if __name__ == "__main__":
    main()
