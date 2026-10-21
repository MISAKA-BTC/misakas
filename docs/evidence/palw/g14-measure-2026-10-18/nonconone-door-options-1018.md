# G14 non-cone door terms: option ceilings and combinations (2026-10-18)

Arithmetic record. No rule, constant, fence, window or price changed; nothing implemented.
Inputs: the shipped door terms of `door-breakdown-1018.md` (fixed 103; trace tiles 4,209; output check 2,346; reorg restart
2,346; owner queue 1,536; descent 369; rows/tile tree 208; IDs 106; non-cone sum 11,120). Script:
`nonconone-door-options-1018.py`; numbers: `nonconone-door-options-1018.json`.

## What this shows

- With the cone term at zero the door is already 11,223. The limit is 3,120, so the non-cone terms must fall by **8,103 DAA
  (73% of 11,120)** before any cone change counts.
- Each non-cone option below is a CEILING on its saving (a term cannot drop below zero, and no residual cost is assumed). The
  ceilings are optimistic. None of the trace-tile, output or reorg residuals has been measured.
- Even when trace tiles, reorg and the owner queue are all removed (ceiling, with the cone at zero), the door is 3,132, which is
  still above 3,120. The output check (2,346) must also shrink. Passing 3,120 therefore needs more than the listed options.

## Table (DAA)

| option | door | <= 3,120 |
|---|---:|:-:|
| current (shipped, 448 serial turns) | 1,062,231 | no |
| cone C only (one round, sparse, body = measured wire) | 12,537 | no |
| cone D only (one round, sparse, body = measured value bytes) | 11,296 | no |
| cone 0 (floor) | 11,223 | no |
| owner queue removed (**decided tenure rule: user decision**) | 9,687 | no |
| output/reorg share authenticated bodies (ceiling: reorg 2,346 saved) | 8,877 | no |
| trace tiles sparse (ceiling: all 4,209 saved) | 7,014 | no |
| trace tiles sparse, 50% saved (illustrative split, not measured) | 9,119 | no |
| combo: cone D + reorg shared + trace 50% | 6,846 | no |
| combo: cone 0, trace 0, reorg 0, queue kept | 4,668 | no |
| combo: cone 0, trace 0, reorg 0, queue 0 (uses the tenure rule change) | 3,132 | no |

## Requirements the options must meet (not met yet)

- Trace tiles: a functional sparse fetch that keeps whole collusion + one external verifier and default-on-missing answers.
- Output and reorg: sharing only for authenticated bodies of the same claim/root/coordinates. Different owners' requests and
  signers' liability or default may not be merged, so the saving may be smaller than the ceiling.
- Owner queue: its removal changes the decided owner-tenure rule. Not implemented; a user decision.
- Slots 2 ("only demanded custodians answer", door 480,184 with the shipped cone) would also drop the default for missing answers
  of the other custodians. This is a rule change and is not listed as an option here.

## Not done

- The residual cost of each option (sparse-fetch overhead, sibling paths, batch framing) is not computed.
- The State and HistTile terms (checkpoint 164, h_tile 32) are not measured, so the cone term for long jobs is not known.
- No option reaches 3,120 under its ceiling without the owner-queue change, so nothing is implemented (see ADR-0189).
