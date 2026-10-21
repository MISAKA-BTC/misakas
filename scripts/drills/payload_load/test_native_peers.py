"""Native RPC-schema fixtures; no connection/node/run is performed."""
import unittest
from native_peers import require_loopback_peers_v1
class NativePeers(unittest.TestCase):
    def test_actual_native_response_shape_and_numeric_loopback_addresses(self):
        response={'peerInfo':[{'address':{'ip':'127.0.0.1','port':60090},
            'advertised_protocol_version':108,'id':'synthetic-schema-fixture','is_outbound':False}]}
        self.assertEqual(require_loopback_peers_v1(response),[('127.0.0.1',60090)])
        self.assertEqual(require_loopback_peers_v1({'peerInfo':[]}),[])
        for ip in ['::1','::ffff:127.0.0.1']:
            self.assertEqual(len(require_loopback_peers_v1({'peerInfo':[{'address':{'ip':ip,'port':33300}}]})),1)
    def test_external_addresses_unknown_schema_missing_fields_and_boolean_ports_refuse(self):
        for value in [None,{}, {'infos':[]},{'peerInfo':None},{'peerInfo':[],'unknown':True},
                      {'peerInfo':['127.0.0.1:33300']},{'peerInfo':[{}]},
                      {'peerInfo':[{'address':'127.0.0.1:33300'}]}]:
            with self.assertRaises(ValueError):require_loopback_peers_v1(value)
        for address in [{'ip':'127.0.0.1'}, {'port':33300},{'ip':'127.0.0.1','port':True},
                        {'ip':'127.0.0.1','port':0},{'ip':'127.0.0.1','port':65536},
                        {'ip':'127.0.0.1','port':'33300'},{'ip':True,'port':33300},
                        {'ip':'localhost','port':33300},{'ip':'::1%lo0','port':33300},
                        {'ip':'127.0.0.1','port':33300,'unknown':1}]:
            with self.assertRaises(ValueError):require_loopback_peers_v1({'peerInfo':[{'address':address}]})
        for ip in ['192.0.2.1','0.0.0.0','::','2001:db8::1','::ffff:192.0.2.1']:
            with self.assertRaises(ValueError):require_loopback_peers_v1({'peerInfo':[{'address':{'ip':ip,'port':33300}}]})
if __name__=='__main__':unittest.main()
