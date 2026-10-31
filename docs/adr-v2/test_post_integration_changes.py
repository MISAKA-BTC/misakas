"""The accepted metadata ledger cannot waive frozen payloads or arbitrary changes."""
import unittest
from check_integrity import metadata_change_matches

class PostIntegrationChanges(unittest.TestCase):
    def setUp(self):
        self.path='docs/rfc-v2/registry.json'
        self.old='a'*64
        self.new='b'*64
        self.change={'path':self.path,'baseline_sha256':self.old,'updated_sha256':self.new,'reason':'RFC-0043 / ADR-0211 adopted'}

    def test_only_exact_before_and_after_are_authorized(self):
        self.assertTrue(metadata_change_matches(self.path,self.old,self.new,self.change))
        self.assertFalse(metadata_change_matches(self.path,self.old,'c'*64,self.change))
        self.assertFalse(metadata_change_matches(self.path,'c'*64,self.new,self.change))

    def test_frozen_sources_and_parameters_cannot_be_waived(self):
        for path in ['docs/adr/0001-example.md','docs/rfc/0002-example.md','docs/governance/parameter-register.md']:
            self.assertFalse(metadata_change_matches(path,self.old,self.new,{**self.change,'path':path}))

    def test_missing_files_or_malformed_digests_cannot_be_authorized(self):
        for actual in [None,'','bad']:
            self.assertFalse(metadata_change_matches(self.path,self.old,actual,{**self.change,'updated_sha256':actual}))
        self.assertFalse(metadata_change_matches(self.path,self.old,self.new,{**self.change,'reason':''}))

if __name__=='__main__':unittest.main()
