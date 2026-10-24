"""POSIX cleanup for finite fixture commands launched in a fresh owned session.

Only this launch's process group is signalled. This does not contain descendants
that deliberately create a new session, and it is not an OS resource quota.
"""
import os
import signal
import subprocess


def start_fixture(argv, stdout, stderr):
    if os.name != 'posix':
        raise RuntimeError('finite fixture process-group cleanup requires POSIX')
    return subprocess.Popen(argv, stdout=stdout, stderr=stderr, text=True, start_new_session=True)


def stop_fixture(child, timeout=20):
    # Signal the owned group even if its leader has exited: a forked descendant
    # may still hold output descriptors and continue writing after poll() != None.
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    # Reap the direct child also when killpg raced its natural exit.
    child.wait(timeout=timeout)
