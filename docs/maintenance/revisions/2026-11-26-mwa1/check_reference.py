#!/usr/bin/env python3
"""MWA1 arithmetic examples only. Not consensus code or executed GPU tests."""
from fractions import Fraction
import unittest

LIMIT=1<<256

def wins(ticket,theta):
    if not 0<=ticket<LIMIT or not 0<=theta<=LIMIT:
        raise ValueError('out of range')
    return ticket<theta

def quote(w,a,b):
    if min(w,a)<0 or b<=0: raise ValueError('bad terms')
    return a*w//b

def apportion(n,weights):
    if n<0 or any(v<0 for v in weights.values()): raise ValueError('bad weight')
    s=sum(weights.values())
    if not s:
        if n: raise ValueError('zero weights')
        return dict.fromkeys(weights,0)
    result={i:n*v//s for i,v in weights.items()}
    rem=n-sum(result.values())
    ranked=sorted((i for i in weights if weights[i]),key=lambda i:(-(n*weights[i]%s),i))
    for i in ranked[:rem]: result[i]+=1
    assert sum(result.values())==n
    return result

def split(c):
    if c<0: raise ValueError('negative')
    f=p=c*50//1000
    a=j=c*200//1000
    d=c-f-p-a-j
    return f,p,a,j,d,d+j

def market(r,v,u,a,j,d):
    b=d+j
    if not b and not a: return 0,0,0,r,u
    if min(r,v,a,j,d)<0 or u<=0 or v+r<=0 or b<=0: raise ValueError('invalid')
    q=b*u//(v+r+b)
    ell=q*j//b
    m=q-ell
    if min(a,j,d,q,ell,m)==0: raise ValueError('dust')
    return q,ell,m,r+b+a,u-m

class ReferenceTests(unittest.TestCase):
    def test_work_ratio(self):
        p=Fraction(1,10)
        self.assertEqual([p*quote(w,1,1) for w in [100,300,1000]],[10,30,100])
        self.assertEqual([p*Fraction(w,100) for w in [100,300,1000]],[Fraction(1,10),Fraction(3,10),1])
    def test_target_boundaries(self):
        self.assertFalse(wins(0,0));self.assertTrue(wins(0,1));self.assertFalse(wins(1,1));self.assertTrue(wins(LIMIT-1,LIMIT))
        for t,h in [(LIMIT,LIMIT),(0,LIMIT+1),(-1,1),(0,-1)]:
            with self.assertRaises(ValueError): wins(t,h)
    def test_floor_error(self):
        for w in range(100):
            exact=Fraction(13*w,7);self.assertTrue(0<=exact-quote(w,13,7)<1)
    def test_budget_max(self):
        for w in range(101): self.assertLessEqual(quote(w,3,2),quote(100,3,2))
        with self.assertRaises(ValueError):quote(1,1,0)
    def test_coupled_apportion(self):
        for c in range(501):
            f,p,a,j,d,b=split(c)
            self.assertEqual(c,f+p+a+b)
            e=d-2*j;self.assertGreaterEqual(e,0)
            for weights in [{'a':1,'b':3,'c':10},{'a':0,'b':1,'c':1},{'a':1,'b':0,'c':0}]:
                js=apportion(j,weights);es=apportion(e,weights)
                ds={i:2*js[i]+es[i] for i in js}
                self.assertEqual(sum(ds.values()),d)
                self.assertEqual(sum(js.values()),a)
                self.assertTrue(all(ds[i]>=2*js[i] for i in js))
                self.assertTrue(all(js[i]==ds[i]==0 for i in js if weights[i]==0))
        self.assertEqual(apportion(1,{'b':1,'a':1}),{'a':1,'b':0})
    def test_market(self):
        f,p,a,j,d,b=split(1000)
        self.assertEqual((f,p,a,j,d,b),(50,50,200,200,500,700))
        q,l,m,r,u=market(1000,0,10_000*100_000_000,a,j,d)
        self.assertEqual(q,l+m);self.assertEqual(r,1900);self.assertEqual(u+m,10_000*100_000_000)
        self.assertGreaterEqual(r*u,1000*10_000*100_000_000)
        self.assertEqual(market(1000,0,100,0,0,0),(0,0,0,1000,100))
        with self.assertRaises(ValueError):market(1000,0,100,1,0,1)
        # Actual purchase budget ratio, not nominal 2/7 at integer residual.
        f,p,a,j,d,b=split(21)
        self.assertEqual((j,d,b),(4,11,15))
        q,l,m,r,u=market(1,0,10_000,a,j,d)
        self.assertEqual(l,q*4//15);self.assertNotEqual(l,q*2//7)
    def test_slash_cumulative(self):
        s=0;reward=0;burn=0
        for delta in [1,3,17,99,400]:
            pay=490*(s+delta)//1000-490*s//1000
            reward+=pay;burn+=delta-pay;s+=delta
            self.assertEqual(reward,490*s//1000);self.assertEqual(s,reward+burn)
    def test_exec_remainder(self):
        for w in range(1000):
            n,r=divmod(w,100)
            self.assertEqual(Fraction(n)+Fraction(r,100),Fraction(w,100))

if __name__=='__main__':unittest.main(verbosity=2)
