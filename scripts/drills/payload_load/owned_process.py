"""Bounded cleanup of Popen objects created by this launcher, never name/port scans."""
import json
import os
import signal
import subprocess
import time

def cleanup_one(child,name,process_group=False,timeouts=(15,15,5)):
    report=dict(name=name,pid=child.pid,actions=[],errors=[])
    for sig,timeout in zip((signal.SIGINT,signal.SIGTERM,signal.SIGKILL),timeouts):
        try:
            code=child.poll()
            if code is not None:
                report.update(return_code=code,still_running=False);return report
            try:
                if process_group:os.killpg(child.pid,sig)
                else:child.send_signal(sig)
                report['actions'].append(dict(signal=sig.name))
            except ProcessLookupError:
                report['actions'].append(dict(signal=sig.name,already_exited=True))
            # Reap even when the signal raced process exit.
            try:
                report['return_code']=child.wait(timeout=timeout)
                report['still_running']=False;return report
            except subprocess.TimeoutExpired:
                report['errors'].append(f'{sig.name}: wait timed out after {timeout}s')
        except BaseException as error:
            # An interrupt or one broken child must not stop cleanup of this or later children.
            report['errors'].append(f'{sig.name}: {type(error).__name__}: {error}')
    try:
        report['return_code']=child.poll();report['still_running']=report['return_code'] is None
    except BaseException as error:
        report['still_running']=None;report['errors'].append(f'final poll: {type(error).__name__}: {error}')
    return report

def cleanup_owned(children,path):
    """children tuples: (owned Popen, label, own process group?, bounded stage waits)."""
    reports=[]
    for child,name,group,timeouts in children:
        try:reports.append(cleanup_one(child,name,group,timeouts))
        except BaseException as error:
            reports.append(dict(name=name,pid=getattr(child,'pid',None),still_running=None,
                                errors=[f'{type(error).__name__}: {error}']))
    # Logging happens after ALL children were attempted, so an IO failure cannot strand the rest.
    try:path.write_text(json.dumps(dict(utc=time.time(),children=reports),indent=2))
    except BaseException as error:reports.append(dict(logging_error=f'{type(error).__name__}: {error}'))
    return reports
