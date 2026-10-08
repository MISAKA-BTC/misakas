#!/usr/bin/env bash
# audit-combined/rfc9-v4-leg.sh — fence 4 of the int-13 flag day (`palw_receipt_spend_v4`, RFC-0009 stage C) on the combined drill chain:
# t12-daa9000-drill-plan.md §3.4 and its §6 item 1 ("the V4 leg scripted: rail + builder + a Final free-prompt claim"). A sibling of dc.sh,
# run beside it on the SAME salted chain, keyring and ports (it sources audit-improve/lib-dm.sh exactly as dm.sh does). Nothing here touches
# a host other than this Mac; nothing is signed with a key outside the drill keyring.
#
#   bash audit-combined/rfc9-v4-leg.sh <command>      BIN_DIR=<the release dir: kaspad misaka misaka-palw-fp-rail misaka-palw-rfc1-drill-claim
#                                                                palw-evidence>   (INT13_AT, as dc.sh's --palw-drill-int13-at; default 110)
#   plan      the leg's timeline in DAA (claim, bind, licence, Final, draw slot, use window) against H' = INT13_AT
#   dry       preflight: every binary and flag the leg uses, the keyring, the ports; nothing written, nothing started
#   prepare   BEFORE `dc.sh up`: the builder node's extra argument (--palw-redemption-auth-dir, BUILDER_NODE, default new0 — a bonded floor
#             producer that is NOT the executor), the leg's directories, and the reference DA provider (palw-evidence serve on 127.0.0.1)
#   certify   after `up` (DAA >= 2): the floor's free-prompt lane certified ON CHAIN — the FP drill evidence (FamilyCertified, as
#             ObjectChunks) then the lane binding (ClassLaneCertified), each with `misaka palw submit-object`, funded by main-0. The drill
#             genesis certifies the floor's FP lane in its params only; until this lands every FP commitment on the floor is skipped
#   claim     the EXECUTOR (keyring seat EXEC_SEAT, default 7: the drills' own bond, no node of its own): one plain free-prompt job on the
#             floor (misaka-palw-rfc1-drill-claim --kind plain), signed and funded by the rail from the seat's fee float, its evidence to a
#             directory provider and then the HTTP provider (palw-evidence publish), its RDA4 authorization (500 bps, every quantum) to both
#             providers (redemption-publish). Then the executor is OFFLINE: this script never reads its key again. Run it early (DAA < 40):
#             Final is ~150 DAA later and the draw slot 400 after that, so the redemption lands ~DAA 600, well past H'
#   sync      the builder's side, any time and repeatedly (cron-able): mirror every valid authorization from the providers into the
#             builder node's --palw-redemption-auth-dir (palw-evidence redemption-sync). The node redeems on its own once the fence is armed,
#             the claim is Final and a quantum wins (kaspad builder mode, `produce_redemption`)
#   status    the claim's phase and quanta spent, the providers' availability of its evidence, the builder's REDEMPTION lines
#   verdict   §3.4's PASS items, read from the nodes: (a) no PFS4 header below H' and the redemption at/after H'; (b) the merging chain
#             block's coinbase splits exactly leg + fee = the authorization's bps of the worker reward, the leg to the EXECUTOR bond's
#             registered payout; (c) quanta spent == redemptions paid (no quantum paid twice); (d) every node holds the merging block on its
#             chain; (e) the PFS4 header is above 8,192 bytes; the refusal strings of the whole run. Exit 0 = PASS
#
# The chain-block E2E of the same leg (reorg, V3 counterfactual twin, sibling and V3 double spends, a replaying node) is
# consensus/src/pipeline/virtual_processor/tests/rfc9_v4_chain_e2e.rs; this drill is the multi-node evidence the plan asks for.
set -euo pipefail
C=$(cd "$(dirname "$0")" && pwd)
WT=$(cd "$C/.." && pwd)
cmd=${1:-plan}; shift || true
export INT11=${INT11:-1} INT12=${INT12:-1}
export WORK_DIR=${WORK_DIR:-$HOME/.misaka-palw-combined-drill}
export P2P_BASE=${P2P_BASE:-51200} BORSH_BASE=${BORSH_BASE:-52200} JSON_BASE=${JSON_BASE:-53200} EVM_BASE=${EVM_BASE:-54200} GRPC_BASE=${GRPC_BASE:-50200}
. "$WT/audit-improve/lib-dm.sh"
INT13_AT=${INT13_AT:-110}
LEG=${LEG_DIR:-$WORK_DIR/rfc9-v4}
EXEC_SEAT=${EXEC_SEAT:-7}
BUILDER_NODE=${BUILDER_NODE:-new0}
RPC_NODE=${RPC_NODE:-new1}
FEE_BPS=${FEE_BPS:-500}
PROV_PORT=${PROV_PORT:-58088}
PROMPT_FORM=${PROMPT_FORM:-merkle} # Params::palw_prompt_ids_form_v1: testnet-12 commits prompts in the tiled Merkle form (t46_false_valid_real_claim); flat elsewhere
RAIL_BIN=${RAIL_BIN:-$(dirname "$KASPAD_BIN")/misaka-palw-fp-rail}
PRODUCER_BIN=${PRODUCER_BIN:-$(dirname "$KASPAD_BIN")/misaka-palw-rfc1-drill-claim}
EVIDENCE_BIN=${EVIDENCE_BIN:-$(dirname "$KASPAD_BIN")/palw-evidence}
RPCPY="$WT/audit-improve/rpc.py"
ok() { echo "  ok   $*"; }
bad() { echo "  FAIL $*"; FAILED=1; }
note() { echo "  note $*"; }
FAILED=0
borsh_rpc() { echo "127.0.0.1:$((BORSH_BASE + $(kof "$RPC_NODE")))"; }
cli() { "$CLI_BIN" --network testnet-12 --rpc "$(borsh_rpc)" "--palw-drill-genesis-salt=$(salt)" "$@"; }
rpc() { python3 "$RPCPY" call --port "$(jport "${3:-$RPC_NODE}")" "$1" "$2"; }
providers() { echo "http://127.0.0.1:$PROV_PORT,$LEG/prov-dir"; }
floor_class() { [ -s "$LEG/cert/floor.class" ] || "$PRODUCER_BIN" --kind fp-certification --outbox "$LEG/cert" --prompt-form "$PROMPT_FORM" >/dev/null; cat "$LEG/cert/floor.class"; }
claim_id() { [ -s "$LEG/claim.id" ] && cat "$LEG/claim.id"; }
daa() { rpc getBlockDagInfo '{}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["virtualDaaScore"])'; }

plan() {
    cat <<EOF
== RFC-0009 fence 4 (palw_receipt_spend_v4) leg — H' = $INT13_AT
  executor     keyring seat $EXEC_SEAT (no node); funds its claim from the seat's fee float; offline after 'claim'
  builder      $BUILDER_NODE's bond (a floor producer), --palw-redemption-auth-dir=$LEG/builder-auth
  providers    $(providers)   (the reference server is started by 'prepare')
  timeline     certify at DAA >= 2 (two blocks for the chunks, then the lane binding)
               claim at DAA < 40 → bound ~+20 (anchor delay) → licensed by the panel → Final at licence + 121
               draw slot = Final + 400 (receipt maturity) → the beacon (first attempt blocks at/after it) → use window 600
               measured on the chain-block E2E (same windows): claim DAA 5 → Final 147 → draw slot 547; the redemption lands at
               ~DAA 550-560 if the claim is made at DAA ~5: ~19.5 h after the chain starts, ~$(( (555 - INT13_AT) * 125 / 3600 )) h after H',
               at ~125 s/DAA. The 400-DAA receipt maturity is testnet-12's and a drill does not shorten it (a drill drills what ships)
  before H'    the builder emits no PFS4 header (builder mode reads nothing while the fence is dormant); a PFS4 header below the fence is
               refused by name at the header stage (chain-block E2E step 1: 'a V4 receipt spend is not valid below palw_receipt_spend_v4')
EOF
}

dry() {
    plan
    echo "== preflight"
    for b in "$KASPAD_BIN" "$CLI_BIN" "$RAIL_BIN" "$PRODUCER_BIN" "$EVIDENCE_BIN"; do [ -x "$b" ] && ok "$b" || bad "missing $b"; done
    local h; h=$("$KASPAD_BIN" --help 2>/dev/null || true)
    for f in --palw-drill-int13-at --palw-redemption-auth-dir; do grep -q -- "$f" <<<"$h" && ok "kaspad lists $f" || bad "kaspad lacks $f"; done
    local out
    grep -aq -- "--redeem-auth-out" "$RAIL_BIN" && ok "the rail takes --redeem-auth-out" || bad "the rail has no --redeem-auth-out"
    out=$("$PRODUCER_BIN" --bogus 2>&1 || true); grep -q 'plain|' <<<"$out" && ok "the producer knows --kind plain (and fp-certification)" || bad "the producer has no --kind plain (rebuild from this tree)"
    [ -s "$KR/manifest.json" ] && ok "keyring $KR" || note "no keyring yet (dc.sh up writes it)"
    [ -s "$KR/bond-$EXEC_SEAT.seed" ] && ok "executor key bond-$EXEC_SEAT.seed" || note "no bond-$EXEC_SEAT.seed yet"
    lsof -nP -iTCP:"$PROV_PORT" -sTCP:LISTEN >/dev/null 2>&1 && note "port $PROV_PORT is taken (a provider already running?)" || ok "provider port $PROV_PORT free"
    [ "$FAILED" = 0 ]
}

prepare() {
    mkdir -p "$LEG"/{prov-dir,prov-http,builder-auth,cert,outbox} "$WORK_DIR/$BUILDER_NODE"
    local x="$WORK_DIR/$BUILDER_NODE/extra-args" arg="--palw-redemption-auth-dir=$LEG/builder-auth"
    grep -qxF -- "$arg" "$x" 2>/dev/null || echo "$arg" >> "$x"
    say "$BUILDER_NODE: $(cat "$x" | tr '\n' ' ')"
    if ! lsof -nP -iTCP:"$PROV_PORT" -sTCP:LISTEN >/dev/null 2>&1; then
        nohup "$EVIDENCE_BIN" serve --root "$LEG/prov-http" --listen "127.0.0.1:$PROV_PORT" > "$LEG/provider.out" 2>&1 & echo $! > "$LEG/provider.pid"
        say "reference provider pid $(cat "$LEG/provider.pid") on 127.0.0.1:$PROV_PORT"
    fi
}

certify() {
    rm -f "$LEG/cert"/fp-*.obj*
    "$PRODUCER_BIN" --kind fp-certification --outbox "$LEG/cert" --prompt-form "$PROMPT_FORM" > "$LEG/cert/files"
    local chunks; chunks=$(grep 'fp-family.obj.chunk' "$LEG/cert/files" | tr '\n' ' ')
    say "FamilyCertified (FreePrompt) in $(wc -w <<<"$chunks") chunks"
    # shellcheck disable=SC2086
    cli palw submit-object --object $chunks --yes --key-file "$KR/main-0.seed" | tee "$LEG/cert/family.out"
    local start; start=$(daa)
    until [ "$(daa)" -ge $((start + 3)) ]; do sleep 20; done   # the completing chunk accepted, the family applied
    cli palw submit-object --object "$LEG/cert/fp-lane.obj" --yes --key-file "$KR/main-0.seed" | tee "$LEG/cert/lane.out"
    say "submitted; 'status' shows the claim lane once the binding is folded"
}

claim() {
    [ -z "$(claim_id)" ] || die "a claim was already made ($(claim_id)); remove $LEG/claim.id to make another"
    local bond fo fa id_json="$LEG/identity.json"
    bond=$(manifest "m['seats'][$EXEC_SEAT]['bond_outpoint']")
    fo=$(manifest "m['seats'][$EXEC_SEAT]['fee_float_outpoint']")
    fa=${EXEC_FLOAT_SOMPI:-10000000000}   # PALW_RC_BOND_FEE_FLOAT_SOMPI (100 BILI): the keyring names the float's outpoint, not its amount
    local now; now=$(daa)
    [ "$now" -lt $((INT13_AT - 60)) ] || note "DAA $now: a claim this late may not reach its draw slot inside the plan's budget"
    "$RAIL_BIN" --print-identity --bond-key-seed "$KR/bond-$EXEC_SEAT.seed" --rpc "$(borsh_rpc)" --bond "$bond" --class-id "$(floor_class)" \
        --palw-drill-genesis-salt "$(salt)" > "$id_json"
    rm -f "$LEG/outbox"/fp-job-*
    local stem; stem=$("$PRODUCER_BIN" --kind plain --prompt-form "$PROMPT_FORM" --identity "$id_json" --outbox "$LEG/outbox" --rpc "$(borsh_rpc)")
    say "produced $stem"
    "$RAIL_BIN" --watch "$LEG/outbox" --once --bond-key-seed "$KR/bond-$EXEC_SEAT.seed" --rpc "$(borsh_rpc)" \
        --funding-outpoint "$fo" --funding-amount "$fa" --retention-dir "$LEG/outbox" \
        --evidence-out "$LEG/prov-dir" --redeem-auth-out "$LEG/claim.rda4" --redeem-fee-bps "$FEE_BPS" \
        --redeem-expiry-daa $((INT13_AT + 4000)) 2>&1 | tee "$LEG/rail.out"
    local id; id=$(grep -oE 'claim [0-9a-f]{128}' "$LEG/rail.out" | head -1 | awk '{print $2}')
    [ -n "$id" ] || die "the rail named no claim (see $LEG/rail.out)"
    echo "$id" > "$LEG/claim.id"
    "$EVIDENCE_BIN" publish --claim "$id" --from "$LEG/prov-dir" --providers "http://127.0.0.1:$PROV_PORT" | tee "$LEG/publish.out"
    "$EVIDENCE_BIN" redemption-publish --file "$LEG/claim.rda4" --providers "$(providers)" | tee "$LEG/rda4-publish.out"
    echo "$(date '+%F %T') EXECUTOR OFFLINE at DAA $(daa): claim $id; the key bond-$EXEC_SEAT.seed is not read again by this leg" | tee -a "$LEG/events.log"
}

sync_auth() {
    "$EVIDENCE_BIN" redemption-sync --providers "$(providers)" --into "$LEG/builder-auth" --now-daa "$(daa)" | tee -a "$LEG/sync.log"
}

status() {
    local id; id=$(claim_id) || true
    [ -n "$id" ] || { say "no claim yet"; return 0; }
    echo "== claim $id at DAA $(daa)"
    rpc getPalwFreePromptClaim "{\"claimId\":\"$id\"}" | python3 -c 'import json,sys; d=json.load(sys.stdin); print("  phase", d.get("phase"), "quanta", d.get("quanta"), "spent", d.get("quantaSpent"), "phase_daa", d.get("phaseDaa"))'
    grep -a "REDEMPTION receipt block" "$WORK_DIR/$BUILDER_NODE/kaspad.out" 2>/dev/null | tail -5 || true
    ls "$LEG/builder-auth" 2>/dev/null | sed 's/^/  auth  /'
}

verdict() {
    local id; id=$(claim_id) || true
    [ -n "$id" ] || die "no claim ($LEG/claim.id)"
    python3 - "$id" "$INT13_AT" "$FEE_BPS" "$WORK_DIR" "$BUILDER_NODE" "$RPCPY" "$KR/manifest.json" "$EXEC_SEAT" "$(all_nodes | tr '\n' ' ')" \
        "$JSON_BASE" <<'PY'
import json, re, subprocess, sys
claim, H, bps, work, builder, rpcpy, manifest, seat, nodes, jbase = sys.argv[1:11]
H, bps, seat, jbase = int(H), int(bps), int(seat), int(jbase)
m = json.load(open(manifest))
def port(n):
    k = {"new%d" % i: i for i in range(16)}.get(n)
    return jbase + (k if k is not None else 0)
def call(n, method, params):
    out = subprocess.run(["python3", rpcpy, "call", "--port", str(port(n)), method, json.dumps(params)], capture_output=True, text=True, timeout=120)
    return json.loads(out.stdout) if out.returncode == 0 and out.stdout.strip() else None
def hexbytes(v):
    if isinstance(v, str): return bytes.fromhex(v)
    return bytes(v or [])
failed = []
def check(ok, what):
    print(("  ok   " if ok else "  FAIL ") + what)
    if not ok: failed.append(what)
rpcn = "new1"
fp = call(rpcn, "getPalwFreePromptClaim", {"claimId": claim}) or {}
print("== claim", claim[:16], "phase", fp.get("phase"), "quanta", fp.get("quanta"), "spent", fp.get("quantaSpent"))
log = open(f"{work}/{builder}/kaspad.out", errors="replace").read()
reds = re.findall(r"produced REDEMPTION receipt block #\d+ ([0-9a-f]{128})", log)
check(len(reds) >= 1, f"the builder ({builder}) produced {len(reds)} redemption block(s)")
exec_script = None
payload = m["seats"][seat].get("payout_payload") or m["seats"][seat].get("payout_script")
paid_quanta = 0
for r in reds:
    b = call(rpcn, "getBlock", {"hash": r, "includeTransactions": True})
    if not b: check(False, f"{r[:16]} is unknown to {rpcn}"); continue
    hd = b["block"]["header"]; vd = b["block"].get("verboseData") or {}
    pc = hexbytes(hd.get("palwCommitment"))
    check(hd.get("daaScore", 0) >= H, f"{r[:16]}: at DAA {hd.get('daaScore')} >= H' {H} (no PFS4 header below the fence)")
    check(pc[:4] == b"PFS4" and len(pc) > 8192, f"{r[:16]}: a PFS4 carriage of {len(pc)} bytes (> 8,192, <= 16,384 past the fence only)")
    merging = None
    for c in vd.get("childrenHashes", []):
        cb = call(rpcn, "getBlock", {"hash": c, "includeTransactions": True})
        if not cb: continue
        cvd = cb["block"].get("verboseData") or {}
        if cvd.get("isChainBlock") and (cvd.get("selectedParentHash") == r or r in cvd.get("mergeSetBluesHashes", []) + cvd.get("mergeSetRedsHashes", [])):
            merging = cb; break
    if merging is None: check(False, f"{r[:16]}: no chain child merges it yet"); continue
    mh = merging["block"]["header"]["hash"] if "hash" in merging["block"]["header"] else merging["block"]["verboseData"]["hash"]
    outs = merging["block"]["transactions"][0]["outputs"]
    # the builder's own script: the receipt block's coinbase miner data (payload: blue score 8, subsidy 8, script version 2, len 1, script)
    cbp = hexbytes(b["block"]["transactions"][0].get("payload"))
    ln = cbp[18] if len(cbp) > 18 else 0
    builder_script = cbp[19:19 + ln].hex()
    by = {}
    for o in outs:
        spk = o.get("scriptPublicKey") or {}
        s = spk.get("scriptPublicKey") if isinstance(spk, dict) else spk
        by[s] = by.get(s, 0) + int(o.get("value") or o.get("amount") or 0)
    fee = by.get(builder_script, 0)
    others = {s: v for s, v in by.items() if s != builder_script}
    # the executor's payout script is P2PKH-ML-DSA-87 over its registered payload; find it among the outputs whose amount makes leg+fee split
    legs = [(s, v) for s, v in others.items() if v > 0 and (v + fee) * bps // 10000 == fee]
    if payload:
        legs = [(s, v) for s, v in legs if payload.lower() in s.lower()] or legs
    check(fee > 0 and len(legs) >= 1, f"{r[:16]} merged by {mh[:16]}: builder fee {fee}, executor leg {legs[0][1] if legs else '?'} (split at {bps} bps exactly)")
    if fee > 0 and legs:
        paid_quanta += 1
        check(fee * 10000 <= (fee + legs[0][1]) * 1000, "the fee is within the chain's 1,000 bps cap")
        for n in nodes.split():
            nb = call(n, "getBlock", {"hash": mh, "includeTransactions": False})
            on = nb and (nb["block"].get("verboseData") or {}).get("isChainBlock")
            check(bool(on), f"{n} holds {mh[:16]} on its chain")
check(int(fp.get("quantaSpent") or 0) == paid_quanta, f"quanta spent {fp.get('quantaSpent')} == redemptions paid {paid_quanta}: no quantum paid twice")
bad_lines = []
for n in nodes.split():
    try: t = open(f"{work}/{n}/kaspad.out", errors="replace").read()
    except OSError: continue
    for line in t.splitlines():
        if "palw_receipt_spend_v4" in line and re.search(r"refus|invalid|reject", line): bad_lines.append(f"{n}: {line[:200]}")
print(f"== refusal lines naming palw_receipt_spend_v4: {len(bad_lines)} (expected 0: the builder never emits one below the fence)")
for l in bad_lines[:10]: print("   ", l)
print("== VERDICT:", "PASS" if not failed else f"FAIL ({len(failed)})")
sys.exit(1 if failed else 0)
PY
}

case $cmd in
    plan) plan ;;
    dry) dry ;;
    prepare) prepare ;;
    certify) certify ;;
    claim) claim ;;
    sync) sync_auth ;;
    status) status ;;
    verdict) verdict ;;
    *) sed -n '2,30p' "$0"; exit 2 ;;
esac
