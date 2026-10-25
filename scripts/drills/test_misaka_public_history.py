#!/usr/bin/env python3
"""Offline synthetic coverage/resume controls; never contacts a public node."""
import argparse
import copy
import json
from pathlib import Path
import tempfile
import io
import gzip
import hashlib
import struct
import types
import unittest
from misaka_public_history_analyze import analyze
from misaka_public_history_collect import prepare, remote_program, BoundedWsRpc, BoundedOutput, REMOTE

G, P, A = ('1'*128, '2'*128, '3'*128)


def block(h, parent=None):
    return {'hash':h,'header':{'hash':h,'daaScore':{G:0,P:1,A:2}[h], 'parentsByLevel':[[parent]] if parent else []},
            'verboseData':{'isChainBlock':True,'selectedParentHash':parent,'palwMergeView':{'roundBlocks':[]},
                           'exec':{'verdict':'unknown'} if h==P else None}}


def stamp():
    return {'node':{'genesisHash':G,'consensusParamsId':'params','consensusScheduleId':'schedule',
                    'importSessionEpoch':0,'importSessionPhase':'legacy-stable','pruningImportStable':True},
            'dag':{'pruningPointHash':G,'sink':A,'virtualDaaScore':2},'anchor':{'block':block(A,P)}}


def page(index,cursor,blocks):
    return {'kind':'page','page':index,'cursor':cursor,'count':len(blocks),'blocks':blocks}


class Coverage(unittest.TestCase):
    def setUp(self):
        self.directory=tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root=Path(self.directory.name)

    def write(self,name,records,tail=b''):
        path=self.root/name
        path.write_bytes(b''.join((json.dumps(r)+'\n').encode() for r in records)+tail)
        return path

    def whole(self):
        return [{'kind':'before','data':stamp()},page(0,G,[block(G),block(P,G),block(A,P)]),{'kind':'after','data':stamp()}]

    def test_closed_ancestry_is_coverage_only_never_compatibility(self):
        r=analyze([self.write('all',self.whole())])
        self.assertEqual(r['coverage_status'],'COMPLETE_TO_ANCHOR')
        self.assertEqual(r['compatibility_status'],'UNKNOWN')
        self.assertEqual(r['observed_node_verdicts'],{'unknown':1})

    def test_segment_resume_inclusive_duplicate_and_tip_advance(self):
        first=[{'kind':'before','data':stamp()},page(0,G,[block(G),block(P,G)]),{'kind':'stopped','reason':'max_pages'},{'kind':'after','data':stamp()}]
        second=[{'kind':'before','data':stamp()},page(0,P,[block(P,G),block(A,P)]),{'kind':'after','data':stamp()}]
        second[-1]['data']['dag']['virtualDaaScore']=100
        second[-1]['data']['dag']['sink']='4'*128
        paths=[self.write('first',first),self.write('second',second)]
        r=analyze(paths)
        self.assertEqual((r['coverage_status'],r['unique_blocks']),('COMPLETE_TO_ANCHOR',3))
        args=argparse.Namespace(ssh_host='misaka-ibm',port=26314,max_pages=2,max_seconds=10,timeout=5,max_frame_bytes=1024,max_segment_bytes=131072,anchor=None,resume=[str(paths[0])],output=str(self.root/'new'))
        cfg=prepare(args)
        self.assertEqual((cfg['anchor'],cfg['cursor']),(A,P))
        compile(remote_program(cfg),'remote-offline-control','exec')
        self.assertFalse((self.root/'new').exists())
        self.assertEqual(paths[0].read_text(), ''.join(json.dumps(r)+'\n' for r in first))

    def test_missing_page_or_ancestor_is_unknown(self):
        records=self.whole();records[1]=page(0,G,[block(G),block(A,P)])
        r=analyze([self.write('hole',records)])
        self.assertEqual(r['ancestor_holes'],[P]);self.assertEqual(r['coverage_status'],'UNKNOWN_PARTIAL')
        records=self.whole();records[1]['page']=1
        self.assertTrue(analyze([self.write('skip',records)])['faults'])

    def test_false_stable_and_changed_identity_or_anchor_refuse(self):
        for field,value in [('pruningImportStable',False),('consensusParamsId','changed'),('consensusScheduleId','changed'),('importSessionEpoch',1),('importSessionPhase','changed'),('genesisHash','changed')]:
            records=self.whole();records[-1]['data']['node'][field]=value
            with self.subTest(field=field):self.assertEqual(analyze([self.write(field,records)])['coverage_status'],'UNKNOWN_PARTIAL')
        records=self.whole();records[-1]['data']['dag']['pruningPointHash']=P
        self.assertEqual(analyze([self.write('ppchanged',records)])['coverage_status'],'UNKNOWN_PARTIAL')
        records=self.whole();records[-1]['data']['anchor']['block']['header']['hash']='4'*128
        self.assertEqual(analyze([self.write('anchor',records)])['coverage_status'],'UNKNOWN_PARTIAL')
        records=self.whole();records[-1]['data']['anchor']['block']['verboseData']['isChainBlock']=False
        self.assertEqual(analyze([self.write('noncanonical',records)])['coverage_status'],'UNKNOWN_PARTIAL')

    def test_error_missing_after_incomplete_and_duplicate_page_are_distinct(self):
        cases={'error':self.whole()+[{'kind':'error','error':'store unavailable'}],
               'noafter':self.whole()[:-1], 'duplicate':self.whole()[:2]+[copy.deepcopy(self.whole()[1])]+self.whole()[2:]}
        for name,records in cases.items():
            with self.subTest(name=name):self.assertEqual(analyze([self.write(name,records)])['coverage_status'],'UNKNOWN_PARTIAL')
        r=analyze([self.write('truncated',self.whole(),b'{"kind":')])
        self.assertEqual(r['coverage_status'],'UNKNOWN_PARTIAL')
        self.assertTrue(any(f['kind']=='incomplete_line' for f in r['faults']))

    def test_round_header_without_decodable_exec_is_not_counted_as_refused(self):
        records=self.whole();records[1]['blocks'][1]['header']['powAlgoId']=10
        records[1]['blocks'][1]['verboseData']['exec']=None
        result=analyze([self.write('noexec',records)])
        self.assertEqual(result['round_header_count'],1)
        self.assertEqual(result['round_headers_without_exec'],[P])
        self.assertEqual(result['observed_node_verdicts'],{})
        self.assertEqual(result['compatibility_status'],'UNKNOWN')

    def test_page_after_terminal_stamp_is_not_closed(self):
        records=self.whole()
        records=[records[0],records[2],records[1]]
        result=analyze([self.write('reordered',records)])
        self.assertEqual(result['coverage_status'],'UNKNOWN_PARTIAL')
        self.assertTrue(any(f['kind']=='page_outside_stamps' for f in result['faults']))

    def test_conflicting_same_hash_is_not_overwritten(self):
        records=self.whole();bad=block(P,G);bad['header']['daaScore']=9
        records[1]['blocks'].append(bad);records[1]['count']+=1
        r=analyze([self.write('conflict',records)])
        self.assertEqual(r['coverage_status'],'UNKNOWN_PARTIAL')
        self.assertTrue(any(f['kind']=='header_conflict_or_invalid' for f in r['faults']))

    def test_resume_rejects_missing_after_and_shell_option_host(self):
        path=self.write('live',self.whole()[:-1])
        args=argparse.Namespace(ssh_host='misaka-ibm',port=26314,max_pages=1,max_seconds=1,timeout=1,max_frame_bytes=1024,max_segment_bytes=131072,anchor=None,resume=[str(path)],output=str(self.root/'new'))
        with self.assertRaises(ValueError):prepare(args)
        args.ssh_host='-oProxyCommand=unsafe'
        with self.assertRaises(ValueError):prepare(args)


    def test_all_verdict_observations_transitions_and_known_conflicts(self):
        records=self.whole()
        granted=block(P,G);granted['verboseData']['exec']={'verdict':'granted','claimId':'later-claim'}
        records[1]['blocks'].insert(2,granted);records[1]['count']+=1
        result=analyze([self.write('transition',records)])
        values=result['verdict_observations'][P]
        self.assertEqual([v['verboseData']['exec']['verdict'] for v in values],['unknown','granted'])
        self.assertEqual(values[1]['verboseData']['exec']['claimId'],'later-claim')
        self.assertEqual(result['verdict_transitions'][P][0]['from'],'unknown')
        self.assertEqual(result['verdict_transitions'][P][0]['to'],'granted')
        self.assertEqual(result['observed_node_verdicts'],{'unknown':1})  # defined first-observation counter
        refused=block(P,G);refused['verboseData']['exec']={'verdict':'refused','refusal':'later-reason'}
        records[1]['blocks'].insert(3,refused);records[1]['count']+=1
        result=analyze([self.write('known-conflict',records)])
        self.assertEqual(result['conflicting_known_verdicts'],[P])
        self.assertEqual(result['coverage_status'],'UNKNOWN_PARTIAL')
        self.assertEqual(result['compatibility_status'],'UNKNOWN')

    def test_oversize_frame_is_refused_before_payload_read(self):
        for marker,extension in [(126,struct.pack('!H',4096)),(127,struct.pack('!Q',2**50))]:
            rpc=object.__new__(BoundedWsRpc);rpc.max_frame_bytes=1024
            reads=[];parts=iter([bytes([0x81,marker]),extension])
            def read(size):
                reads.append(size)
                value=next(parts)
                self.assertEqual(size,len(value))
                return value
            rpc._read=read
            with self.assertRaisesRegex(ValueError,'max_frame_bytes'):rpc._recv_frame()
            self.assertEqual(reads,[2,len(extension)])
        rpc=object.__new__(BoundedWsRpc);rpc.max_frame_bytes=3
        parts=iter([b'\x81\x03',b'abc']);rpc._read=lambda size:next(parts)
        self.assertEqual(rpc._recv_frame(),(1,b'abc'))

    def test_actual_collector_byte_stop_keeps_unsaved_cursor_and_after_stamp(self):
        config={'port':26314,'timeout':1,'anchor':A,'identity':None,'anchor_header':None,'cursor':None,
                'max_pages':100,'max_seconds':100,'max_frame_bytes':131072,'max_segment_bytes':1114112}
        output=io.BytesIO();calls=[]
        class FakeRpc:
            def __init__(self,**kwargs):pass
            def close(self):pass
            def call(inner,method,params=None):
                if method=='getPalwNodeStatus':return stamp()['node']
                if method=='getBlockDagInfo':return stamp()['dag']
                if method=='getBlock':return stamp()['anchor']
                self.assertEqual(method,'getBlocks')
                cursor=params['lowHash'];calls.append(cursor)
                number=int(cursor,16);following=format(number+1,'0128x')
                # Real-sized bounded response with immutable duplicate cursor header.
                blocks=[]
                for h in [cursor,following]:
                    blocks.append({'header':{'hash':h,'daaScore':1,'parentsByLevel':[], 'palwCommitment':'x'*48000},'verboseData':{}})
                response={'blockHashes':[cursor,following],'blocks':blocks}
                self.assertLess(len(json.dumps(response).encode()),config['max_frame_bytes'])
                return response
        from misaka_public_history_analyze import IDENTITY, stamp_identity
        scope={'BoundedWsRpc':FakeRpc,'BoundedOutput':BoundedOutput,'config':config,'IDENTITY':IDENTITY,
               'stamp_identity':stamp_identity,'sys':types.SimpleNamespace(stdout=types.SimpleNamespace(buffer=output))}
        with self.assertRaises(SystemExit) as exit_value:exec(REMOTE,scope)
        self.assertEqual(exit_value.exception.code,0,output.getvalue().decode()[-3000:])
        self.assertLessEqual(len(output.getvalue()),config['max_segment_bytes'])
        records=[json.loads(line) for line in output.getvalue().splitlines()]
        stopped=next(r for r in records if r['kind']=='stopped')
        pages=[r for r in records if r['kind']=='page']
        self.assertTrue(pages)
        self.assertEqual(stopped['reason'],'max_segment_bytes')
        self.assertEqual(stopped['cursor'],pages[-1]['blocks'][-1]['hash'])
        self.assertEqual(calls[-1],stopped['cursor'])
        self.assertEqual(records[-1]['kind'],'after')
        self.assertTrue(records[-1]['data']['node']['pruningImportStable'])


    def test_gzip_roundtrip_preserves_original_byte_identity_and_resume_cursor(self):
        records=[{'kind':'before','data':stamp()},page(0,G,[block(G),block(P,G)]),
                 {'kind':'stopped','reason':'max_pages'},{'kind':'after','data':stamp()}]
        plain=self.write('first.ndjson',records)
        compressed=self.root/'first.ndjson.gz'
        compressed_bytes=gzip.compress(plain.read_bytes(),mtime=0)
        compressed.write_bytes(compressed_bytes)
        left,right=analyze([plain]),analyze([compressed])
        self.assertEqual(left['segments'][0]['sha256'],right['segments'][0]['sha256'])
        self.assertEqual(left['segments'][0]['observed_bytes'],right['segments'][0]['observed_bytes'])
        self.assertEqual(right['segments'][0]['storage_sha256'],hashlib.sha256(compressed_bytes).hexdigest())
        self.assertEqual(right['segments'][0]['storage_bytes'],len(compressed_bytes))
        self.assertEqual(left['last_complete_page_cursor'],right['last_complete_page_cursor'])
        args=argparse.Namespace(ssh_host='misaka-ibm',port=26314,max_pages=1,max_seconds=1,timeout=1,
            max_frame_bytes=1024,max_segment_bytes=131072,anchor=None,resume=[str(compressed)],output=str(self.root/'next.ndjson'))
        self.assertEqual(prepare(args)['cursor'],P)
        completed=self.write('whole.ndjson',self.whole())
        (self.root/'whole.ndjson.gz').write_bytes(gzip.compress(completed.read_bytes(),mtime=0))
        self.assertEqual(analyze([self.root/'whole.ndjson.gz'])['coverage_status'],'COMPLETE_TO_ANCHOR')
        self.assertEqual(analyze([self.root/'whole.ndjson.gz'])['compatibility_status'],'UNKNOWN')

    def test_gzip_corrupt_crc_and_truncation_are_unknown_and_resume_refused(self):
        plain=self.write('whole.ndjson',self.whole()).read_bytes()
        data=gzip.compress(plain,mtime=0)
        corrupted=bytearray(data);corrupted[-8]^=1
        for name,payload in [('crc',bytes(corrupted)),('truncated',data[:-1]),('shortfooter',data[:-8])]:
            with self.subTest(name=name):
                path=self.root/(name+'.ndjson.gz');path.write_bytes(payload)
                result=analyze([path])
                self.assertEqual(result['coverage_status'],'UNKNOWN_PARTIAL')
                self.assertTrue(any(f['kind']=='compression_or_storage_integrity' for f in result['faults']))
                args=argparse.Namespace(ssh_host='misaka-ibm',port=26314,max_pages=1,max_seconds=1,timeout=1,
                    max_frame_bytes=1024,max_segment_bytes=131072,anchor=None,resume=[str(path)],output=str(self.root/'next.ndjson'))
                with self.assertRaises(ValueError):prepare(args)


if __name__=='__main__':unittest.main()
