"""Hypothetical UTXO observations, never node activity or funded coins."""
import copy, unittest
from launch import matching_funded_outputs
class Launch(unittest.TestCase):
    def test_funding_rpc_txid_alone_never_passes_actual_output_readiness(self):
        txid='a'*128;script='000076c440'+'b'*128+'88a6'
        row=dict(outpoint=dict(transactionId=txid,index=0),utxoEntry=dict(amount=1000*100_000_000,
            scriptPublicKey=script,blockDaaScore=5,isCoinbase=False))
        check=lambda rows:matching_funded_outputs(dict(entries=rows),txid,script,1000*100_000_000,5)
        self.assertEqual(check([]),[]);self.assertEqual(len(check([row])),1)
        for field,value in [('amount',1),('scriptPublicKey','0000'),('blockDaaScore',6),('isCoinbase',True)]:
            bad=copy.deepcopy(row);bad['utxoEntry'][field]=value;self.assertEqual(check([bad]),[])
        bad=copy.deepcopy(row);bad['outpoint']['transactionId']='c'*128;self.assertEqual(check([bad]),[])
        bad=copy.deepcopy(row);bad['outpoint']['index']=-1;self.assertEqual(check([bad]),[])
if __name__=='__main__':unittest.main()
