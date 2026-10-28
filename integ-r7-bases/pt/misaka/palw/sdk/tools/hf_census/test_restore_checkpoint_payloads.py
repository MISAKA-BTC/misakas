"""Source restoration never installs wrong bytes or replaces an existing source file."""
import hashlib,io,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
from restore_checkpoint_payloads import restore
class Response(io.BytesIO):
 status=200
 def __init__(self,data):super().__init__(data);self.headers={'Content-Length':str(len(data))}
class Restore(unittest.TestCase):
 def row(self,root):
  data=b'checkpoint';return {'id':'one','repository':'owner/model','revision':'a'*40,'source_directory':str(root)}, {'path':'model.safetensors','bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()},data
 def test_pinned_payload_installs_only_after_full_hash(self):
  with tempfile.TemporaryDirectory() as temp:
   root=Path(temp);m,r,data=self.row(root)
   with patch('urllib.request.urlopen',return_value=Response(data)):
    result=restore(m,r)
   self.assertEqual(result['status'],'RESTORED_PASS');self.assertEqual((root/r['path']).read_bytes(),data)
   self.assertEqual(list(root.glob('*.part')),[])
 def test_existing_wrong_file_is_not_overwritten_or_fetched(self):
  with tempfile.TemporaryDirectory() as temp:
   root=Path(temp);m,r,data=self.row(root);(root/r['path']).write_bytes(b'private existing content')
   with patch('urllib.request.urlopen') as remote:result=restore(m,r);remote.assert_not_called()
   self.assertEqual(result['status'],'EXISTING_MISMATCH_NOT_OVERWRITTEN');self.assertEqual((root/r['path']).read_bytes(),b'private existing content')
 def test_wrong_hash_and_oversized_response_never_install(self):
  for data in [b'wrongbytes',b'checkpoint plus unbounded data']:
   with tempfile.TemporaryDirectory() as temp:
    root=Path(temp);m,r,_=self.row(root)
    with patch('urllib.request.urlopen',side_effect=lambda *a,**kw:Response(data)):result=restore(m,r)
    self.assertEqual(result['status'],'RESTORE_FAILED');self.assertFalse((root/r['path']).exists())
if __name__=='__main__':unittest.main()
