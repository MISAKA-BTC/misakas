"""Fake owned Popen fixtures; no process/node is launched or signalled."""
import json
import signal
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from owned_process import cleanup_one,cleanup_owned
class Child:
    def __init__(self,pid,outcomes):self.pid=pid;self.outcomes=iter(outcomes);self.code=None;self.signals=[];self.waits=[]
    def poll(self):return self.code
    def send_signal(self,sig):self.signals.append(sig)
    def wait(self,timeout):
        self.waits.append(timeout);outcome=next(self.outcomes)
        if isinstance(outcome,BaseException):raise outcome
        self.code=outcome;return outcome
class Cleanup(unittest.TestCase):
    def test_timeouts_escalate_owned_group_to_kill_and_reap(self):
        child=Child(101,[subprocess.TimeoutExpired('fixture',1),subprocess.TimeoutExpired('fixture',2),-9])
        with patch('owned_process.os.killpg') as kill:
            result=cleanup_one(child,'fixture',True,(1,2,3))
        self.assertEqual([args.args for args in kill.call_args_list],[(101,signal.SIGINT),(101,signal.SIGTERM),(101,signal.SIGKILL)])
        self.assertEqual(child.waits,[1,2,3]);self.assertFalse(result['still_running']);self.assertEqual(result['return_code'],-9)
    def test_one_failure_or_interrupt_does_not_skip_later_owned_children(self):
        stuck=Child(102,[RuntimeError('bad wait'),KeyboardInterrupt(),subprocess.TimeoutExpired('fixture',3)])
        observer=Child(103,[0])
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'cleanup.json'
            reports=cleanup_owned([(stuck,'sender',False,(1,2,3)),(observer,'observer',False,(35,10,5))],path)
            self.assertTrue(reports[0]['still_running']);self.assertFalse(reports[1]['still_running'])
            self.assertEqual(observer.signals,[signal.SIGINT]);self.assertEqual(observer.waits,[35])
            self.assertEqual(len(json.loads(path.read_text())['children']),2)
    def test_log_failure_happens_after_all_children_and_graceful_exit_is_preferred(self):
        first=Child(104,[0]);second=Child(105,[0])
        class BrokenLog:
            def write_text(self,_):raise OSError('synthetic disk full')
        reports=cleanup_owned([(first,'a',False,(1,1,1)),(second,'b',False,(1,1,1))],BrokenLog())
        self.assertEqual(first.signals,[signal.SIGINT]);self.assertEqual(second.signals,[signal.SIGINT])
        self.assertIn('logging_error',reports[-1])
if __name__=='__main__':unittest.main()
