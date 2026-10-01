#!/usr/bin/env bash
# audit-gen/dg.sh — the generative drill's scenarios (RFC-0003 activation step 8), written as functions over lane C's
# drill harness (audit-tir/lib-df.sh's conventions: `cli <node> …`, `jport`, `manifest`, `salt`, `say`/`die`, $WORK_DIR,
# $KR, $UHOME, nodes.sh start|stop). Source it from the combined harness, or run it standalone on a chain brought up
# by audit-tir/df.sh: DG_LIB=<that harness's lib> bash audit-gen/dg.sh <command>.
#
#   classes   verify the Plan B artifacts in $GEN_DIR (written by `PALW_GEN_DRILL_OUT=$GEN_DIR cargo test -p
#             misaka-palw-sdk --test gen_drill_classes -- --ignored`) and print their ids
#   dg1       the three crossings: a gen registration dropped by name below GEN_AT and accepted from it; a v10 claim
#             skipped below FPV5_AT and accepted from it; the wide class refused (PALW-GEN-21) below HELD_AT, accepted
#             from it. (The old relay's refusal past GEN_AT is lane C's D-M5.)
#   dg2       the readiness gate: the embedding class registered late (LATE_AT), a claim at FPV5_AT is not taken
#             (GenClassNotReady) and is taken once >= 5 operators are ready
#   dg3       honest claims to Final: the image claim (new5) and the embedding claim (new6)
#   dg4       the one-move court: bond 10 plants a cone lie in the denoiser, bond 11 an output lie; both convicted
#   dg5       the caps: nine unlicensed claims from one bond, the ninth refused (BondClassShareExceeded)
#   dg6       the held leaf challenge (tag 90): bond 12 lies in the wide class; the refuting seat declares the close and
#             delivers its chunks; the claim is convicted. KILL=1 is DG-7a: the accusing seat is stopped between the
#             declaration and the last chunk and restarted inside the assembly clock; it resumes and the close completes
#   dg7b      the lapse: bond 13 lies in the wide class; the accusing seat is kept down past 4*count DAA; the declarer is
#             charged and the claim is not convicted
#   status    the generative claims of every drill bond
#
# Heights (lane C's int-11 layout): GEN_AT=28 FPV5_AT=104 HELD_AT=140. Per-claim identities (nonce, seed) derive from the tag.
set -euo pipefail
G=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)
. "${DG_LIB:-$G/../audit-tir/lib-df.sh}"
GEN_DIR=${GEN_DIR:-$WORK_DIR/gen}
GEN_AT=${GEN_AT:-28}; FPV5_AT=${FPV5_AT:-104}; HELD_AT=${HELD_AT:-140}; LATE_AT=${LATE_AT:-103}
DG_NODE=${DG_NODE:-new3}                 # the node whose JSON port is read
IMAGE_EXEC=${IMAGE_EXEC:-new5}; EMBED_EXEC=${EMBED_EXEC:-new6}
SEAT_NODES=${SEAT_NODES:-"new0 new1 new2 new3 new4 new5 new6"}
FAILED=0
ok() { echo "  ok   $*"; }
bad() { echo "  FAIL $*"; FAILED=1; }
dgw() { python3 "$G/dgwatch.py" --port "$(jport "$DG_NODE")" "$@"; }
at_daa() { while [ "$(dgw tip)" -lt "$1" ]; do sleep 10; done; }

classes_manifest() { [ -s "$GEN_DIR/drill-classes.json" ] || die "no $GEN_DIR/drill-classes.json (see the header: gen_drill_classes --ignored)"; }
class_field() { python3 -c "import json,sys; m=json.load(open('$GEN_DIR/drill-classes.json'))['classes']; print(next(c for c in m if c['name']==sys.argv[1])[sys.argv[2]])" "$1" "$2"; }
class_file() { echo "$GEN_DIR/$1.class.palwtir2"; }
seat_field() { manifest "m['seats'][$1]['$2']"; }          # bond_outpoint, operator_id (the keyring manifest's)
liar_field() { python3 -c "import json; print(json.load(open('$WORK_DIR/liars/bond-$1.json'))['$2'])"; }  # bond_outpoint, seed_file, operator_id
retention_of() { ls -d "$WORK_DIR/$1"/app/*/palw-retention 2>/dev/null | head -1; }
seat_of() { case $1 in new0) echo 3;; new1) echo 0;; new2) echo 1;; new3) echo 2;; new4) echo 4;; new5) echo 5;; new6) echo 6;; esac; }

classes() {
    classes_manifest
    for c in toy-image toy-embed wide-embed; do
        [ -s "$(class_file "$c")" ] && ok "$c: $(class_field "$c" bytes) bytes, class $(class_field "$c" class_id | cut -c1-16)… root $(class_field "$c" artifact_root | cut -c1-16)…" \
            || bad "$c: $(class_file "$c") missing"
    done
    [ "$FAILED" = 0 ]
}

# gen_register <class> <node> — a signed registration from the node's seat bond, submitted.
gen_register() {
    local c=$1 n=$2 seat; seat=$(seat_of "$n")
    local obj=$GEN_DIR/$c.registration.obj
    cli "$n" palw gen-registration --artifact "$(class_file "$c")" --bond "$(seat_field "$seat" bond_outpoint)" \
        --key-file "$KR/bond-$seat.seed" --out "$obj" >/dev/null
    cli "$n" palw submit-object --key-file "$KR/bond-$seat.seed" --object "$obj" --yes >/dev/null
    say "registration of $c filed from $n at DAA $(dgw tip)"
}

# gen_request <class> <tag> — a gen-claim request from the class's template, anchored on the chain's sink.
gen_request() {
    local c=$1 tag=$2 out=$GEN_DIR/req-$2.json a
    a=$(dgw anchor)
    python3 - "$GEN_DIR/drill-classes.json" "$c" "$tag" "$a" "$out" <<'PY'
import hashlib, json, sys
path, name, tag, anchor, out = sys.argv[1:6]
sink, daa = anchor.split()
t = next(c for c in json.load(open(path))["classes"] if c["name"] == name)["gen_claim_request_template"]
t = dict(t)
t["anchor_block"], t["anchor_daa"] = sink, int(daa)
t["job_nonce"] = hashlib.sha256(("nonce/" + tag).encode()).hexdigest()
t["seed"] = hashlib.sha256(("seed/" + tag).encode()).hexdigest()
json.dump(t, open(out, "w"), indent=1)
PY
    echo "$out"
}

# gen_claim <node> <class> <bond-outpoint> <seed-file> <operator-id> <tag> [--plant SPEC] — runs the job, files the claim.
gen_claim() {
    local n=$1 c=$2 bond=$3 seed=$4 op=$5 tag=$6; shift 6
    mkdir -p "$GEN_DIR/claims"
    local req out id
    req=$(gen_request "$c" "$tag")
    out=$(cli "$n" palw gen-claim --artifact "$(class_file "$c")" --request "$req" --bond "$bond" --operator-id "$op" \
        --key-file "$seed" --out-dir "$GEN_DIR/claims" --retention-dir "$(retention_of "$n")" "$@")
    echo "$out" | sed 's/^/    /' >&2
    id=$(echo "$out" | sed -n 's/^claim \([0-9a-f]*\):.*/\1/p' | head -1)
    [ -n "$id" ] || die "gen-claim printed no claim id"
    cli "$n" palw fp-submit --tx "$GEN_DIR/claims/$id.commitment-tx.borsh" --yes >/dev/null
    say "claim $(echo "$id" | cut -c1-16)… ($c, tag $tag) filed from $n at DAA $(dgw tip)"
    echo "$id"
}

# leaf_of <class> <stage> <kind-prefix> — the global index of the first leaf of a stage whose kind starts with a prefix,
# from a gen-claim --list-leaves run (nothing is filed).
leaf_of() {
    local c=$1 stage=$2 kind=$3 f=$GEN_DIR/leaves-$1.json
    [ -s "$f" ] || { local req; req=$(gen_request "$c" "list-$c"); \
        cli "$IMAGE_EXEC" palw gen-claim --artifact "$(class_file "$c")" --request "$req" --bond "$(seat_field "$(seat_of "$IMAGE_EXEC")" bond_outpoint)" \
            --operator-id "$(seat_field "$(seat_of "$IMAGE_EXEC")" operator_id)" --key-file "$KR/bond-$(seat_of "$IMAGE_EXEC").seed" \
            --out-dir "$GEN_DIR/claims" --list-leaves "$f" >/dev/null; }
    python3 -c "import json,sys; L=json.load(open('$f'))['leaves']; print(next(l['global'] for l in L if l['stage']==int(sys.argv[1]) and l['kind'].startswith(sys.argv[2]) and l['lanes']>0))" "$stage" "$kind"
}

dg1() {
    classes_manifest
    say "DG-1: below GEN_AT=$GEN_AT the gen registration is dropped by name"
    at_daa $((GEN_AT - 4)); gen_register toy-image "$IMAGE_EXEC"
    at_daa $((GEN_AT + 3))
    dgw class --class "$(class_field toy-image class_id)" >/dev/null && bad "the registration below the fence was taken" || ok "dropped by name below $GEN_AT"
    gen_register toy-image "$IMAGE_EXEC"; sleep 60
    at_daa $((GEN_AT + 8)); dgw class --class "$(class_field toy-image class_id)" >/dev/null && ok "accepted from $GEN_AT" || bad "not listed from $GEN_AT"
    say "DG-1: a v10 claim below FPV5_AT=$FPV5_AT is skipped, from it accepted"
    local s; s=$(seat_of "$IMAGE_EXEC")
    at_daa $((FPV5_AT - 4)); local early; early=$(gen_claim "$IMAGE_EXEC" toy-image "$(seat_field "$s" bond_outpoint)" "$KR/bond-$s.seed" "$(seat_field "$s" operator_id)" dg1-early)
    at_daa $((FPV5_AT + 4))
    dgw claims --bond "$(seat_field "$s" bond_outpoint)" --class "$(class_field toy-image class_id)" | python3 -c "import json,sys; d=json.load(sys.stdin); sys.exit(0 if not any(c['claim']==sys.argv[1][:16] for c in d['claims']) else 1)" "$early" \
        && ok "skipped below $FPV5_AT" || bad "the early claim was taken"
    say "DG-1: the wide class is refused (PALW-GEN-21) below HELD_AT=$HELD_AT"
    at_daa $((HELD_AT - 3)); gen_register wide-embed "$IMAGE_EXEC"
    at_daa $((HELD_AT + 4))
    dgw class --class "$(class_field wide-embed class_id)" >/dev/null && bad "the wide class was registered below $HELD_AT" || ok "refused below $HELD_AT"
    gen_register wide-embed "$IMAGE_EXEC"; at_daa $((HELD_AT + 10))
    dgw class --class "$(class_field wide-embed class_id)" >/dev/null && ok "accepted from $HELD_AT" || bad "the wide class is not listed after $HELD_AT"
    [ "$FAILED" = 0 ] && echo "DG-1 PASS" || { echo "DG-1 FAIL"; return 1; }
}

dg2() {
    classes_manifest
    say "DG-2: the embedding class registered at $LATE_AT; a claim at $FPV5_AT meets GenClassNotReady"
    at_daa "$LATE_AT"; gen_register toy-embed "$EMBED_EXEC"
    local s; s=$(seat_of "$EMBED_EXEC")
    at_daa "$FPV5_AT"
    local id; id=$(gen_claim "$EMBED_EXEC" toy-embed "$(seat_field "$s" bond_outpoint)" "$KR/bond-$s.seed" "$(seat_field "$s" operator_id)" dg2-early)
    at_daa $((FPV5_AT + 6))
    grep -qh "GenClassNotReady" "$WORK_DIR"/new*/kaspad.out 2>/dev/null && ok "refused: GenClassNotReady" || bad "no GenClassNotReady in the nodes' logs (the claim $id)"
    say "DG-2: ready operators accumulate; a claim is taken once five distinct operators hold the class"
    local deadline=$((FPV5_AT + 60))
    while [ "$(dgw tip)" -lt "$deadline" ]; do
        id=$(gen_claim "$EMBED_EXEC" toy-embed "$(seat_field "$s" bond_outpoint)" "$KR/bond-$s.seed" "$(seat_field "$s" operator_id)" "dg2-$(dgw tip)") && break
        sleep 120
    done
    dgw wait --daa "$(( $(dgw tip) + 6 ))" >/dev/null
    dgw claims --bond "$(seat_field "$s" bond_outpoint)" --class "$(class_field toy-embed class_id)" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['counts']); sys.exit(0 if d['claims'] else 1)" \
        && ok "the embedding claim is taken once ready" || bad "no embedding claim taken by DAA $deadline"
    [ "$FAILED" = 0 ] && echo "DG-2 PASS" || { echo "DG-2 FAIL"; return 1; }
}

dg3() {
    classes_manifest
    local si ei; si=$(seat_of "$IMAGE_EXEC"); ei=$(seat_of "$EMBED_EXEC")
    at_daa "$FPV5_AT"
    gen_claim "$IMAGE_EXEC" toy-image "$(seat_field "$si" bond_outpoint)" "$KR/bond-$si.seed" "$(seat_field "$si" operator_id)" dg3-image >/dev/null
    say "DG-3: waiting for the image claim's Final (licence + the 120-DAA window)"
    dgw wait --bond "$(seat_field "$si" bond_outpoint)" --class "$(class_field toy-image class_id)" --final 1 --deadline-daa "${DG3_DEADLINE_DAA:-300}" \
        && ok "the image claim reached Final" || bad "the image claim did not reach Final by ${DG3_DEADLINE_DAA:-300}"
    gen_claim "$EMBED_EXEC" toy-embed "$(seat_field "$ei" bond_outpoint)" "$KR/bond-$ei.seed" "$(seat_field "$ei" operator_id)" dg3-embed >/dev/null
    dgw wait --bond "$(seat_field "$ei" bond_outpoint)" --class "$(class_field toy-embed class_id)" --final 1 --deadline-daa "${DG3_DEADLINE_DAA:-300}" \
        && ok "the embedding claim reached Final" || bad "the embedding claim did not reach Final by ${DG3_DEADLINE_DAA:-300}"
    [ "$FAILED" = 0 ] && echo "DG-3 PASS" || { echo "DG-3 FAIL"; return 1; }
}

dg4() {
    classes_manifest
    local l10=$(leaf_of toy-image 1 "commit") out_lane=0
    say "DG-4: bond 10 plants a cone lie at the denoiser's leaf $l10; bond 11 an output lie (lane $out_lane)"
    local ex=$IMAGE_EXEC
    gen_claim "$ex" toy-image "$(liar_field 10 bond_outpoint)" "$(liar_field 10 seed_file)" "$(liar_field 10 operator_id)" dg4-cone --plant "step:$l10:0:3" >/dev/null
    gen_claim "$ex" toy-image "$(liar_field 11 bond_outpoint)" "$(liar_field 11 seed_file)" "$(liar_field 11 operator_id)" dg4-output --plant "output:$out_lane:1" >/dev/null
    for b in 10 11; do
        dgw wait --bond "$(liar_field $b bond_outpoint)" --class "$(class_field toy-image class_id)" --convicted 1 --deadline-daa "${DG4_DEADLINE_DAA:-$(( $(dgw tip) + 80 ))}" \
            && ok "bond $b: the lying claim was convicted" || bad "bond $b: not convicted"
    done
    [ "$FAILED" = 0 ] && echo "DG-4 PASS" || { echo "DG-4 FAIL"; return 1; }
}

dg5() {
    classes_manifest
    local s i; s=$(seat_of "$IMAGE_EXEC")
    for i in $(seq 1 9); do
        gen_claim "$IMAGE_EXEC" toy-image "$(seat_field "$s" bond_outpoint)" "$KR/bond-$s.seed" "$(seat_field "$s" operator_id)" "dg5-$i" >/dev/null || true
        sleep 5
    done
    at_daa $(( $(dgw tip) + 8 ))
    grep -qh "BondClassShareExceeded" "$WORK_DIR"/new*/kaspad.out 2>/dev/null && ok "the ninth unlicensed claim was refused: BondClassShareExceeded" || bad "no BondClassShareExceeded in the nodes' logs"
    [ "$FAILED" = 0 ] && echo "DG-5 PASS" || { echo "DG-5 FAIL"; return 1; }
}

# the node that filed a held leaf challenge, from the nodes' logs ("filing it as a held leaf challenge … session S")
accuser_of_held() { grep -l "filing it as a held leaf challenge" "$WORK_DIR"/new*/kaspad.out 2>/dev/null | head -1 | xargs -n1 dirname | xargs -n1 basename; }

dg6() {
    classes_manifest
    local bond=${LIAR_BOND:-12} tag=dg6
    say "DG-6${KILL:+ (with the DG-7a kill)}: bond $bond lies in the wide class"
    at_daa "$HELD_AT"
    gen_claim "$EMBED_EXEC" wide-embed "$(liar_field "$bond" bond_outpoint)" "$(liar_field "$bond" seed_file)" "$(liar_field "$bond" operator_id)" "$tag" --plant "step:0:0:3" >/dev/null
    if [ -n "${KILL:-}" ]; then
        local who=""
        for _ in $(seq 1 120); do who=$(accuser_of_held || true); [ -n "$who" ] && break; sleep 10; done
        [ -n "$who" ] || { bad "no seat filed a held leaf challenge"; echo "DG-6 FAIL"; return 1; }
        say "DG-7a: stopping the accusing seat $who between the declaration and the last chunk"
        bash "$A/nodes.sh" stop "$who"; sleep 20; bash "$A/nodes.sh" start "$who"
        sleep 60
        grep -q "delivering what is missing" "$WORK_DIR/$who/kaspad.out" && ok "$who resumed delivering after its restart" || note "no resume line (the close may have completed before the stop)"
    fi
    dgw wait --bond "$(liar_field "$bond" bond_outpoint)" --class "$(class_field wide-embed class_id)" --convicted 1 --deadline-daa "${DG6_DEADLINE_DAA:-$(( $(dgw tip) + 100 ))}" \
        && ok "the wide-class lie was convicted through the chunked close" || bad "the wide-class lie was not convicted"
    [ "$FAILED" = 0 ] && echo "DG-6 PASS" || { echo "DG-6 FAIL"; return 1; }
}

dg7b() {
    classes_manifest
    local bond=${LIAR_BOND:-13} tag=dg7b
    say "DG-7b: bond $bond lies in the wide class; the accusing seat is kept down past the assembly clock"
    gen_claim "$EMBED_EXEC" wide-embed "$(liar_field "$bond" bond_outpoint)" "$(liar_field "$bond" seed_file)" "$(liar_field "$bond" operator_id)" "$tag" --plant "step:0:0:5" >/dev/null
    local who=""
    for _ in $(seq 1 120); do who=$(accuser_of_held || true); [ -n "$who" ] && break; sleep 10; done
    [ -n "$who" ] || { bad "no seat filed a held leaf challenge"; echo "DG-7b FAIL"; return 1; }
    local seat before after; seat=$(seat_of "$who")
    before=$(dgw claims --bond "$(seat_field "$seat" bond_outpoint)" | python3 -c "import json,sys; print(json.load(sys.stdin)['slashed'] or 0)")
    bash "$A/nodes.sh" stop "$who"
    at_daa $(( $(dgw tip) + ${DG7B_DOWN_DAA:-16} ))
    bash "$A/nodes.sh" start "$who"; at_daa $(( $(dgw tip) + 6 ))
    after=$(dgw claims --bond "$(seat_field "$seat" bond_outpoint)" | python3 -c "import json,sys; print(json.load(sys.stdin)['slashed'] or 0)")
    [ "$after" -gt "$before" ] && ok "the declarer $who was charged ($before -> $after)" || bad "the declarer was not charged"
    dgw claims --bond "$(liar_field "$bond" bond_outpoint)" --class "$(class_field wide-embed class_id)" | python3 -c "import json,sys; d=json.load(sys.stdin); sys.exit(0 if d['counts']['convicted']==0 else 1)" \
        && ok "the lying claim was NOT convicted by the lapsed close" || bad "the lying claim was convicted"
    [ "$FAILED" = 0 ] && echo "DG-7b PASS" || { echo "DG-7b FAIL"; return 1; }
}

status() {
    for n in $SEAT_NODES; do local s; s=$(seat_of "$n"); echo "$n: $(dgw claims --bond "$(seat_field "$s" bond_outpoint)" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['counts'], 'collateral', d['collateral'], 'slashed', d['slashed'])")"; done
    for b in 10 11 12 13; do [ -s "$WORK_DIR/liars/bond-$b.json" ] && echo "liar $b: $(dgw claims --bond "$(liar_field $b bond_outpoint)" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['counts'], d['void_reasons'])")"; done
}

if [ "${BASH_SOURCE[0]:-$0}" = "$0" ]; then
    cmd=${1:-}; shift || true
    case $cmd in
        classes|dg1|dg2|dg3|dg4|dg5|dg6|dg7b|status) "$cmd" "$@" ;;
        *) sed -n '2,27p' "$0"; exit 2 ;;
    esac
fi
