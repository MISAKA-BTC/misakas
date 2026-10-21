"""Local temporary files and mocked capability subprocess; no network or funds."""
import argparse, hashlib, json, os, subprocess, tempfile, unittest
from pathlib import Path
from unittest.mock import patch
from driver import prepare
class Driver(unittest.TestCase):
    def test_capabilities_and_hash_are_required_before_a_sender_command_is_formed(self):
        with tempfile.TemporaryDirectory() as directory:
            work=Path(directory);work.chmod(0o700)
            key=work/'synthetic.seed';key.write_text('synthetic fixture only');key.chmod(0o600)
            sender=work/'sender';sender.write_text('not an executable measurement binary')
            manifest=work/'manifest.json'
            manifest.write_text(json.dumps(dict(network='devnet',key_origin='isolated-devnet-measurement',
                sender_sha256=hashlib.sha256(sender.read_bytes()).hexdigest(),source_commit='synthetic-test')))
            a=argparse.Namespace(work=work,key=key,sender=sender,manifest=manifest,tps=1,seconds=600,
                grpc_port=30111,payload_bytes=17408,fanout_timeout=3600)
            old=dict(key_scheme='MLDSA87',trace_version=0,devnet_fanout=False)
            with patch('driver.subprocess.run',return_value=subprocess.CompletedProcess([],0,json.dumps(old),'') ):
                with self.assertRaises(ValueError): prepare(a)
            caps=dict(key_scheme='MLDSA87',trace_version=1,devnet_fanout=True)
            with patch('driver.subprocess.run',return_value=subprocess.CompletedProcess([],0,json.dumps(caps),'') ) as run:
                _,command,_,_=prepare(a)
                self.assertIn('--initial-utxos',command);self.assertIn('600',command)
                self.assertEqual(run.call_args.args[0],[str(sender.resolve()),'--measurement-capabilities'])
                sender.write_text('changed binary')
                with self.assertRaises(ValueError): prepare(a)
                self.assertEqual(run.call_count,1,'wrong hash refuses before executing the binary')
                a.fanout_timeout=3601
                with self.assertRaises(ValueError): prepare(a)
                self.assertEqual(run.call_count,1,'timeout bound refuses before executing the binary')
if __name__=='__main__':unittest.main()
