"""Run a command as a child in a session of its own, and stop whatever it leaves there.

Shared by factory/bin/check-lock, factory/bin/test-run and factory/bin/nightly, which import it from
beside themselves.

handle_stops() makes SIGTERM and SIGINT stop the run: everything still running in the command's
session gets SIGTERM, and the process exits 128 + signal. Install it before anything else, so that a
stop is never missed. A stop that arrives while the command is being started, before its pid is
known, is remembered and honoured the moment it is. run() starts the command, waits for it, stops
what it left in its session and returns its code (128 + signal when a signal killed it). A killpg
that finds the group already gone never changes that code. start() starts it the same way and returns
the process, for a caller that talks to it (test-run's ssh to the gate VM).
"""

import os
import signal
import sys
from subprocess import Popen

child = None
starting = False
stopped = 0


def leftovers():
    if child is None:
        return
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except (ProcessLookupError, PermissionError):
        # macOS answers EPERM, not ESRCH, when the group's last process has just ended.
        pass


def stop(signum, _):
    global stopped
    if starting:
        # The command is being started and isn't known yet: run stops it once it is.
        stopped = signum
        return
    leftovers()
    sys.exit(128 + signum)


def handle_stops():
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)


def start(command, **popen):
    global child, starting
    starting = True
    child = Popen(command, start_new_session=True, **popen)
    starting = False
    if stopped:
        stop(stopped, None)
    return child


def run(command, **popen):
    code = start(command, **popen).wait()
    leftovers()
    return 128 - code if code < 0 else code
