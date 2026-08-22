# testnet-12 — a seat node deadlocked in place, 2026-08-22

Recorded because the evidence does not survive the fix: the process was killed to recover the
node, and the host has no `gdb`/`eu-stack`/`pstack`, so **no userspace backtrace exists and the
cause is not identified.** What follows is what was measured before the kill.

## What happened

Host C (`5.104.81.23`), the seat holding **bond 2** (`--palw-panel`, appdir `/root/.t12`,
listening `:26411`), stopped doing anything at **06:55:25 CEST** and was still stopped when it was
found at **07:39** — 44 minutes. Its two siblings on the same host (`.t12b`, `.t12c`) logged
continuously throughout, so this was one process, not the host.

The process was alive (`State: S`) and never exited on `SIGTERM`: it was still there after 60 s
and needed `SIGKILL`. A healthy node of this build exits on `SIGTERM` in seconds.

## The measurements

**It stopped reading its listening socket.** 34 sockets on `:26411` in `CLOSE-WAIT`, most with
**226 bytes unread** in the receive queue — a peer's version message, delivered by the kernel and
never taken by the application. This is what made it visible: a node being brought up on host B
could establish TCP and then hang forever, because C is the only fleet member B's egress reaches.

**Every thread was parked.** 44 threads, 42 in `futex_wait_queue`:

```
  8 tokio-runtime-w     futex_wait_queue
  8 rocksdb:low         futex_wait_queue
  3 kaspad              futex_wait_queue
  2 rocksdb:high        futex_wait_queue
  1 virtual-process     futex_wait_queue
  8 virtual-pool-0..7   futex_wait_queue
  1 pruning-process     futex_wait_queue
  1 pipe_read · 1 hrtimer_nanosleep
```

The parked set spans the virtual-processing pool **and** the rocksdb threads **and** every tokio
worker — nothing was left to make progress.

> **A caveat that matters, learned the same hour.** An *idle* node looks identical at this level:
> parked tokio workers sit in `futex_wait_queue` too. The new node on host B was read as
> deadlocked on exactly this evidence and was not — it was idle for want of a reachable peer, and
> resumed the moment C came back. `futex_wait_queue` across all threads is **not** on its own a
> diagnosis. What made C's case real was the three facts around it: no log output for 44 minutes
> while siblings logged, an accept queue that was never drained, and `SIGTERM` doing nothing.
> The cheap discriminator is `/proc/<pid>/stat` utime+stime over a 20 s window — B's advanced by
> 1,222 ticks, which is not a deadlock.

## The last thing it said

`rusty-kaspa_err.log`, once every ~2 s up to 06:55:24:

```
[palw-panel] a quorum stands but no fee UTXO resolves — a carrier may still be in
flight; else fund --palw-fee-outpoint
```

then, in the main log, a final burst at 06:55:25.24x–.271:

```
recovery-trace stage=Rejected attempt=None candidate=Some(e103cd02…)
recovery-trace stage=Rejected attempt=None candidate=Some(351047ce…)
```

and nothing further. Both patterns are present on healthy nodes too, so neither is the cause on
its own — they are recorded as the state the process was in when it stopped.

## Recovery, and what it does not tell us

Restarted with the identical command line; the node came back and has logged normally since.
**A restart is not a diagnosis** — the same conditions produced the same two log patterns again
within seconds of coming up, so nothing here says it will not recur.

Two things would make the next occurrence answerable, and neither is in place:

1. **A stack tool on the fleet hosts.** One `eu-stack -p <pid>` would have named the lock. Install
   `elfutils` (or `gdb`) on all four before the next run.
2. **A liveness check that watches the accept queue.** "The log has not moved and `CLOSE-WAIT` is
   climbing on the P2P port" is a precise, cheap signal, and it is what a watcher should alert on
   — the process being alive and the port being open both looked fine here.

## Fleet state at the time

t10 and t11 had just been stopped on all four hosts, and **that is not the cause**: this node went
quiet at 06:55:25 and the first change on host C was at ~07:05. The producer on `169.58.39.220`
was unaffected throughout and was at block #763 when this was written.
