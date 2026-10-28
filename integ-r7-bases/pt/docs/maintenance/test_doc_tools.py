import sys
sys.dont_write_bytecode=True
import unittest
from doclib import spans,resolve,anchors,key
class DocTests(unittest.TestCase):
 def urls(self,s):return [u for a,b,u in spans(s)]
 def test_inline(self):self.assertEqual(self.urls('[a](../x.md#h)'),['../x.md#h'])
 def test_fence(self):self.assertEqual(self.urls('```sh\n[a](bad)\n```\n[x](ok)'),['ok'])
 def test_tilde(self):self.assertEqual(self.urls('~~~\n[a](bad)\n~~~\n[x](ok)'),['ok'])
 def test_code(self):self.assertEqual(self.urls('`[a](bad)` [x](ok)'),['ok'])
 def test_comment(self):self.assertEqual(self.urls('<!-- [a](bad) --> [x](ok)'),['ok'])
 def test_angle(self):self.assertEqual(self.urls('[a](<a file.md>)'),['a file.md'])
 def test_reference(self):self.assertEqual(self.urls('[x][id]\n[id]: a.md "title"'),['a.md'])
 def test_parentheses(self):self.assertEqual(self.urls('[a](a(b).md)'),['a(b).md'])
 def test_html(self):self.assertEqual(self.urls('<a href="a.md">X</a>'),['a.md'])
 def test_code_label(self):self.assertEqual(self.urls('[`x`](a.md)'),['a.md'])
 def test_offsets(self):
  t='before [x](a.md) after'
  for a,b,u in spans(t):self.assertEqual(t[a:b],u)
 def test_relative(self):self.assertEqual(resolve('docs/a/b.md','../c.md#x'),('local','docs/c.md','x'))
 def test_absolute(self):self.assertEqual(resolve('docs/a.md','/docs/b.md'),('local','docs/b.md',''))
 def test_external(self):self.assertEqual(resolve('docs/a.md','https://example.com')[0],'external')
 def test_anchor(self):self.assertTrue({'hello','hello-1','exact'}.issubset(anchors('# Hello\n## Hello\n<a id="exact"></a>')))
 def test_key_no_line(self):self.assertEqual(key(dict(status='missing-doc-file',source='a',target='b',line=1)),key(dict(status='missing-doc-file',source='a',target='b',line=2)))
if __name__=='__main__':unittest.main()
