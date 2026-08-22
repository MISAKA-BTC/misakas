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

Two things would make the next occurrence answerable. **Both are now in place** (2026-08-22):

1. **A stack tool on the fleet hosts.** `elfutils` 0.190 installed on all four; `eu-stack -p <pid>`
   walks a live node's 44 threads in **128 ms**.
2. **A liveness watcher on the accept queue** — `scripts/palw-liveness-watch`, running as
   `palw-liveness-watch.service` on all four hosts (enabled, 60 s poll). It fires on *stalled log
   for >240 s* **AND** *(≥5 CLOSE-WAIT **or** a non-empty LISTEN backlog)* on that node's P2P port,
   and captures a bundle: `eu-stack`, thread census, sockets, `/proc` status, CPU delta and log
   tails. It does **not** restart anything — a restart is what made the first occurrence
   unanswerable.

### What the drills found, and why the watcher is not the first draft

The watcher was exercised against a real node (`.t12b` on ibm, `SIGSTOP`/`SIGCONT`) rather than
declared working, and two defects fell out of doing that:

* **`sockets.txt` came back as a bare header.** The capture used `ss -tn`, which lists only
  ESTABLISHED — so the file recording the alert's own evidence was empty. Fixed to `ss -tanp`,
  plus a separate CLOSE-WAIT-only file.
* **CLOSE-WAIT alone is not a dependable trigger.** It only accumulates once peers give up, so on
  a quiet network a wedged node could sit at zero. `Recv-Q` on the **LISTEN** socket is the same
  fault one step earlier — it *is* the accept backlog — and is now an independent trigger. Drilled
  on its own with a single connection and no peer traffic: fired at 60 s with `close_wait/listen_backlog=1/1`.

### The stripped-binary problem, and what the capture does about it

The shipped `kaspad` has **no `.symtab` and no `.debug_*`**, so `eu-stack` prints raw addresses.
The bundle therefore also records `/proc/<pid>/maps`, the Build ID
(`752a3e1cf827bd3776e6466e460b47189919f03f`) and the binary's sha256, and pre-translates every
frame to `<module>+0x<vaddr>` in `eu-stack-resolved.txt` — 299 frames resolved in the drill.
Names still need an unstripped build of the same commit; `HOW-TO-RESOLVE.txt` in each bundle says
so, and says to match the Build ID first, because a different build resolves to confident
nonsense.

## Fleet state at the time

t10 and t11 had just been stopped on all four hosts, and **that is not the cause**: this node went
quiet at 06:55:25 and the first change on host C was at ~07:05. The producer on `169.58.39.220`
was unaffected throughout and was at block #763 when this was written.


---

## The second occurrence, same day — and the watcher's blind spot it exposed

The relaunched ibm seat (`.t12b`, bond-1, `:26421`) froze at **10:41:55 CEST**, at the exact
moment it connected to the producer and logged `Registering p2p flows for peer … protocol
version 103` — mid-handshake, five re-syncs into the relaunched chain. It sat frozen for 5.5
hours. Thread census identical to the morning's C incident (every pool parked in
`futex_wait_queue`), CPU 7 ticks over 10 s, `State: S`. **This time a stack exists**:
`eu-stack` walked all 44 threads (526 lines, 325 frames resolved to `kaspad+0x…` offsets
against Build ID `75439aee…`'s maps) — captured live, before any restart, which is the whole
reason elfutils was installed this morning. Bundle: `/var/log/palw-wedge/manual-*-t12b/` on ibm,
mirrored to the session scratchpad.

**And the watcher missed it for 5.5 hours, by design.** Both socket triggers assume somebody
knocks: CLOSE-WAIT accumulates only when peers dial in and give up; the LISTEN backlog fills
only when connections arrive. Every fleet peer connects OUTBOUND from this seat, so a frozen
accept loop produced no signal at all — log stalled, sockets silent, watcher quiet. The third
wedge signature is *a wedged node nobody dials*.

Closed by making the watcher **knock for itself**: when the log is stalled past the threshold
and both socket signals are absent, it opens one self-connection to the node's own P2P port and
holds it a few seconds. The kernel completes the handshake either way; a healthy node's accept
loop drains it within milliseconds, a wedged one leaves it sitting in the LISTEN backlog — which
is exactly the trigger the morning drill already validated, now self-supplied. Deployed to all
four hosts, and validated against the REAL wedge rather than a drill: the probing watcher fired
on `.t12b` within one poll cycle (`/var/log/palw-wedge/20260822T141528Z-p26421/`).

Downstream effect worth recording: with this seat frozen, `ReceiptLicensed` submissions on the
relaunched chain stopped at 13:02 (587 in the 10:00 hour, then one, then nothing), so the
licensing loop's stall traces to seat liveness, not to the fee floats — the genesis-float fix
did its job (the 10:45 burst through the funded submitter is the drill loop closing live).
The freeze context (`protocol version 103` flow registration) matches the IBD-flow work in
flight on the branch at the time of writing.


---

## The fleet-deployment blockers, measured (2026-08-22 16:30 CEST)

Both were carried as named operational blockers. Measured directly, neither is what it was
called, and the difference matters because one of them is not fixable from a shell at all.

| carried as | measured | disposition |
|---|---|---|
| "host A の egress filter" — A cannot reach the fleet on 26411 | A → ibm reaches **:22, :26411, :26421 and :443, all OK**; A → C :26411 OK | **Not an egress filter.** The BLOCKED reading reproduces only while the far-side node is DOWN — a closed port, not a filtered one. It was taken during a window when ibm's seats were stopped for the re-mint. |
| "C の ufw 26411 未開放" | `ufw allow 26411/tcp` returns *"Skipping adding existing rule"*; `ufw status` shows the rule for v4 and v6 | **Already open**, and had been. The original observation truncated `ufw status` before the line. |

What IS filtered, and was not the item on the list: **inbound from the public internet to A and
B**. From an outside host, `:26411` answers on ibm and C and does not on A (`ufw` inactive, node
listening) or B (`ufw` inactive, node stopped by us). Host-level firewalling is not doing it, so
it is upstream — a provider security group — and it is the one item here that needs a console,
not a shell. A and B therefore participate by dialing OUT, which is exactly the topology the
launch record already describes (ibm-and-C as the reachable pair).

The practical consequence for the drill: a six-seat fleet works, because A dials out and the two
reachable hosts carry inbound. It is not a public-entry topology, and nothing measured today
changes that — it stays an operator item with a name that now matches what it is.
