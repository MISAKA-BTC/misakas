# Eligibility and the lottery — design

> **Not normative.** This document explains why the rules in
> [spec/palw/06-eligibility-and-block-production.md](../../spec/palw/06-eligibility-and-block-production.md)
> are what they are.

**Decisions recorded in:** ADR-0038, 0039, 0044 D4, 0055, 0071, 0072, 0074, 0083, 0117, 0123, 0132,
0137, 0138, 0141, 0142, and ADR-0144 §4/P6.
**Last revised:** 2026-09-27

## 1. Problem

Eligibility must be scarce and assigned by the protocol (P6). It must also be unknowable when the
work is chosen (§4), so a miner cannot draw a ticket first and then spend it on the cheapest job.
Grinding must cost real inference, never free hashing. And the chain needs a clock that keeps moving
when nobody wins.

## 2. The design in one paragraph

The ticket is a hash of the execution commitment, over priced bytes in which every field is pinned or
is the challenge, so one draw is one execution. A free-prompt quantum draws from a beacon that did
not exist when the claim was fixed. Since the single lottery, the class ticket against one network
work target is the whole lottery, and no production lane is priced by `bits`. The DAA score on
testnet-12 therefore ticks with the heartbeat cursor: one slot per interval, with missed slots lost.

## 3. Alternatives considered and rejected

| Alternative | Why rejected | Where recorded |
| --- | --- | --- |
| A ticket per nonce over one execution | Free grinding | archive 0072 §1 |
| Freezing the attempt price off `bits` | Removed the only control on block interval (41–54 blocks/min) | archive 0071 §3 |
| Two draws (the class ticket plus the network PoW) | 79 % of forwards were thrown away | archive 0132 |
| A validator-drawn attempt | A trusted party; the beacon is the chain's own | archive 0074 §1 |
| An inference as the ticket with no hash at all | Open question, decided nothing | archive 0141 |

## Source texts (archived ADR bodies)

- [ADR-0038: PALW is the consensus work — sampled-verified LLM PoW, a receipt-licensed weight ramp, and a hash anti-stall floor](archive/0038-palw-is-the-consensus-work.md)
- [ADR-0039: PALW-only block production — a Base class instead of a hash floor, and a two-weight fork choice](archive/0039-palw-only-block-production.md)
- [ADR-0071 — The attempt lane's price, the ticket's bound, and who may judge a class](archive/0071-the-attempt-lanes-price-and-the-tickets-bound.md)
- [ADR-0072 — The ticket is the execution: both lotteries priced in inferences](archive/0072-the-ticket-is-the-execution.md)
- [ADR-0074: The attempt is a claim, drawn by the chain](archive/0074-the-attempt-is-a-claim-drawn-by-the-chain.md)
- [ADR-0141 — Can an inference be the ticket without a hash lottery?](archive/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md)
