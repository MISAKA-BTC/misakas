"""Recompute the private Studio settlement from the registration market, Borsh book and native checkpoint roots."""
import argparse
import hashlib
import io
import json
import re
from pathlib import Path

from check_fresh_certificate import studio_economics
from check_legacy_work import require


class Reader(io.BytesIO):
    def take(self, n):
        data = self.read(n)
        require(len(data) == n, "truncated MOE2 checkpoint")
        return data

    def num(self, width=8):
        return int.from_bytes(self.take(width), "little")

    def digest(self):
        return self.take(64).hex()

    def expect(self, n, width):
        require(self.num(width) == n, f"unexpected Borsh count or tag at {self.tell()-width}")

    def market(self):
        initial = ("opened_daa", "msk_reserve", "position_units", "sold_units", "burned_sompi", "registrant_paid_sompi")
        value = {k: self.num() for k in initial}
        self.expect(2, 1)  # Open market with a serialized virtual reserve (ADR-0162).
        value.update(closed_to_buys=False, contributor_paid_sompi=self.num(), seed_sompi=self.num(), seeded_by=self.digest())
        value.update({k: self.num() for k in ("buyback_sompi", "retired_units", "virtual_sompi")})
        value["seed_pledged_sompi"] = value["seed_sompi"]
        return value


def keyed(domain, data):
    return hashlib.blake2b(data, digest_size=64, key=domain).hexdigest()


def verify(root):
    path = root / "moe2-studio.log"
    log = path.read_text()
    [binary] = re.findall(r"^test_binary_sha256=([0-9a-f]{64})$", log, re.M)
    peers = studio_economics(path, binary)
    model, artifact = peers["model"], peers["inventory"]
    fields = dict(re.findall(r"(\w+): ([0-9a-f]{128})", re.search(r"MOE2 accounting projection pid=\d+ Observation \{ (.*?) \}", log)[1]))
    raw = (root / "settled.borsh").read_bytes()
    undo = (root / "disconnect.borsh").read_bytes()
    require(keyed(b"misaka-palw/private/moe2/store/v1", bytes.fromhex(peers["settlement_child"]) + raw) == fields["checkpoint"],
            "book differs from the three peers' checkpoint")
    book = Reader(raw)
    book.expect(1, 4)  # models
    require(book.digest() == book.digest() == model and book.digest() == artifact, "different registered model")
    registered = book.num()
    book.expect(1, 1)
    book.take(68)  # additional model registrant outpoint
    book.expect(1, 1)
    current = book.market()
    book.expect(0, 4)  # consumed admission
    book.expect(1, 4)  # settled claims
    claim = book.digest()
    miner, panel = book.num(), book.num()
    book.expect(1, 1)
    buy, lp, workers, purchased, locked, burned = (book.num() for _ in range(6))
    require(book.market() == current and (book.num(), book.num()) == (miner, panel), "allocation differs from paid rights")
    book.expect(0, 1)  # vesting
    require(book.digest() == keyed(b"misaka-palw/private/moe2/settlement-undo/v1", undo), "different disconnect journal")
    book.expect(1, 4)  # locked liquidity receipts
    require(book.digest() == book.digest() == claim, "receipt does not name the funded claim")
    policy = book.digest()
    require(book.digest() == model and book.digest() == artifact, "different receipt model")
    position = book.take(64+8*3+4)
    require(position[:64].hex() == fields["position"], "different canonical settlement position")
    before, after = book.market(), book.market()
    require(tuple(book.num() for _ in range(5)) == (buy, lp, purchased, locked, burned), "receipt allocation mismatch")
    for identity in (model, artifact):
        book.expect(1, 4)
        require(book.digest() == identity, "missing permanent LP exclusion")
    book.expect(1, 1)
    require(book.take(len(position)) == position and not book.read(), "different last settlement or trailing checkpoint bytes")

    terms = Reader(undo)
    require(terms.digest() == claim, "different reserved claim")
    terms.take(68)
    require((terms.digest(), terms.digest(), terms.digest(), terms.digest()) == (model, artifact, policy, peers["registration"]),
            "different reserved terms or quote anchor")
    start, deadline = terms.num(), terms.num()
    reward, max_purchase, min_out, max_units, max_lp, max_locked = (terms.num() for _ in range(6))
    terms.expect(1, 1)  # MinerAndPanel20
    slippage, impact = terms.num(2), terms.num(4)
    require(keyed(b"misaka-palw/private/moe2/reward-reservation/v1", undo[:terms.tell()]) == fields["reservation"],
            "different pre-agreed economic terms")
    require(start == registered == before["opened_daa"] and start <= int.from_bytes(position[64:72], "little") <= deadline,
            "outside original quote window")
    market_lines = re.findall(r" market=PalwModelMarketV1 \{ (.*?) \}", log)
    require(len(market_lines) == 3, "missing native registration market")
    for line in market_lines:
        observed = dict(pair.split(": ") for pair in line.split(", "))
        require(all(observed[k] == str(v).lower() for k, v in before.items()), "receipt price differs from native registration")
    require(reward == peers["reservation_sompi"] and (buy, lp) == (3*reward//10, reward//5) and workers == reward-buy-lp and
            panel == workers//5 and miner == workers-panel and (miner,panel) == (peers["miner_sompi"],peers["panel_sompi"]),
            "draw, market split or worker distribution mismatch")
    anchor_x = before["msk_reserve"] + before["virtual_sompi"]
    x = anchor_x + buy
    y = before["position_units"]
    require(purchased == buy*y//x and locked == lp*(y-purchased)//x and burned == purchased-locked and
            buy <= max_purchase and lp <= max_lp and min_out <= purchased <= max_units and 0 < locked <= max_locked,
            "purchase, permanent units or excess burn differs from the registered curve")
    require(slippage <= 10_000 and purchased*10_000 >= (buy*y//x)*(10_000-slippage) and
            buy*y*10_000 <= purchased*anchor_x*(10_000+impact), "signed slippage or price impact exceeded")
    carriers = re.findall(r"^MOE2 pid=(\d+) checkpoint_carrier=(\w+) marker=(\w+) native_debit_and_market=true detached_checkpoint_rejected=true$", log, re.M)
    require(sorted(int(pid) for pid, _, _ in carriers) == sorted(peers["processes"]) and
            {carrier for _, carrier, _ in carriers} == {fields["carrier"]} and len({marker for _, _, marker in carriers}) == 1,
            "peers did not bind the same checkpoint carrier")
    require(after == current == dict(before, msk_reserve=before["msk_reserve"]+buy+lp, position_units=y-burned,
                                     retired_units=before["retired_units"]+burned, buyback_sompi=before["buyback_sompi"]+buy),
            "pool or unrelated market balance differs after atomic settlement")
    return dict(binary_sha256=binary, readers=peers["processes"], model=model, claim=claim, checkpoint=fields["checkpoint"],
                settlement_position=fields["position"], native_projected_root=peers["settlement_child"],
                reward_sompi=reward, purchase_sompi=buy, permanent_lp_sompi=lp, purchased_units=purchased,
                permanent_lp_units=locked, excess_burned_units=burned, miner_sompi=miner, panel_sompi=panel)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    print(json.dumps(verify(parser.parse_args().root), indent=2))
