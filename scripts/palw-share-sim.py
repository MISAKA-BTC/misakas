#!/usr/bin/env python3
"""PALW model-share designs, side by side, as a fluid (expected-value) simulation with integer
block semantics where they matter (the class DAA reads block COUNTS).

Time is stepped one span at a time (SPAN_S seconds, BPS blocks per second). A model class m is the
compute of one forward (CCU_m, MAC-eq — the registry's `economic_ccu_per_claim`), the forwards per
second its producers can run (r_m, the compute SUPPLY), and the leaves of one inference (pwu_m, what
ADR-0076's seed reads). The panel is ONE network-wide pool of ready seats: every claim of class m
costs the pool CCU_m x PANEL_SEATS of replay, claims drain oldest first across classes, and a claim
that waits longer than its class's window voids (receipt_timeout). Supply is given relative to what
the panel can verify (`--supply-x`), because on testnet-11 verification, not compute, binds.

Designs
  A0  the shipped rule: shares from admission (ADR-0135 D5: admission = min(budget/CCU, capacity),
      x 50/100/1000 permille by state), class targets walked by the class DAA (ADR-0071/0076:
      expected = share x blocks, clamped x4 per epoch, a silent class converges toward the hardest
      producing price), a registration copies the floor's target (op 180), epoch budgets (ADR-0039
      D5) floored at one block per class, the reward is the escrow of the block (ADR-0124 80/20).
  A1  A0 + the interim seating price (804af11d: attempt_target_seed_v1(share, pwu) at admission)
      + Upgrade C (min(escrow, attempted x rate), ADR-0132 C).
  D   the global work target: p_m = CCU_m / W, W = max(W0, C / BPS) walked by one DAA over MODEL
      blocks, the floor fills the cadence remainder unpaid, every model block pays the escrow; no
      class share, no class target, no epoch budget; a class is gated only by its panel (readiness
      and an in-flight cap the producer pre-checks).
  E   D with the ticket scaled by the class's verification headroom instead of a hard cap.
  B   D with the floor inside the same lottery (W walked over ALL blocks, floor paid nothing) and no
      residual rule: shows why the floor must be the residual.

Every design walks the same scenarios; the printed table is what ADR-0137 quotes.
"""
from __future__ import annotations

import argparse
import math
from dataclasses import dataclass, field

BPS = 1.0
SPAN_S = 5.0
BLOCKS_PER_SPAN = BPS * SPAN_S
EPOCH_SPANS = 20
EPOCH_BLOCKS = BLOCKS_PER_SPAN * EPOCH_SPANS
ESCROW = 2_756.0  # MSK per block, testnet-11's worker carve (ADR-0132 measured 2,756.28)
PANEL_SEATS = 5
MAX_FACTOR = 4.0  # class_daa_max_factor, every shipped bundle
FLOOR_MIN_PERMILLE = 20  # min_base_class_share_permille on the devnet fence
REFERENCE_WORK_PER_SPAN = 4e9 * SPAN_S  # globals.reference_work_per_span, per seat
UTILIZATION = 0.7  # globals.utilization_permille
BUDGET_CCU_PER_SPAN = 10 * 166_204_342_272  # globals.budget_ccu_per_span
PROBATION_CLAIMS = 10
STABLE_SPANS = 3
UNIT_SHARE_PWU = float(1 << 31)  # PALW_ATTEMPT_TARGET_UNIT_SHARE_PWU_V1
FLOOR_P_T11 = 3.8e-5  # the floor's ticket on testnet-11 (26,403 expected attempts, ADR-0131)
RATE_MAX_MSK_PER_G = 9.0  # Upgrade C rate on the arming branch: 9 MSK per 1e9 CCU
W0 = ESCROW / RATE_MAX_MSK_PER_G * 1e9  # the least work a block buys under D: 306 G MAC-eq
READY_SEATS = 8

CCU = {"Q25": 84.65e9, "Q36": 21.07e9, "Kimi": 300e9, "S": 0.5e9}
PWU = {"Q25": 1_589_424, "Q36": 2_685_360, "Kimi": 6_000_000, "S": 20_000}


@dataclass
class Model:
    name: str
    kind: str
    supply: float  # forwards per second
    registered_at: int = 0
    state: str = "Registered"
    since: int = 0
    probes: float = 0.0
    stable: int = 0
    p: float = 0.0
    share: float = 0.0
    priced: bool = False
    budget_used: float = 0.0
    epoch_blocks: float = 0.0
    inflight: float = 0.0
    hist: dict = field(default_factory=lambda: {"msk": [], "ccu": [], "final_ccu": [], "blocks": [], "claims": [], "p": [], "state": [], "finals": []})

    @property
    def ccu(self) -> float:
        return CCU[self.kind]

    @property
    def pwu(self) -> float:
        return PWU[self.kind]

    @property
    def window(self) -> int:
        return math.ceil(self.ccu * 2.0 / REFERENCE_WORK_PER_SPAN) + 1

    @property
    def admission_per_span(self) -> float:
        """ADR-0135's admission_claims_per_span: min(budget / CCU, inflight cap / window)."""
        per_claim = self.ccu * PANEL_SEATS
        offered_per_seat = REFERENCE_WORK_PER_SPAN * UTILIZATION
        seats = max(PANEL_SEATS + 2, math.ceil(per_claim / offered_per_seat))
        inflight_cap = max(1.0, seats * self.window * offered_per_seat / per_claim)
        return min(BUDGET_CCU_PER_SPAN / self.ccu, inflight_cap / self.window)


ADMISSION_PERMILLE = {"Probation": 50, "ActiveLimited": 100, "Active": 1000}


class Sim:
    def __init__(self, design: str, models: list[Model], spans: int, ready_seats: int = READY_SEATS, hold: bool = True, floor_supply: float = 600.0):
        self.design = design
        self.models = models
        self.spans = spans
        self.hold = hold
        self.p_net_draw = design in ("A0", "A1")
        self.floor_supply = floor_supply
        self.p_net = 1.0
        self.p_floor = FLOOR_P_T11
        self.W = W0
        self.ready_seats = ready_seats
        self.panel_per_span = ready_seats * REFERENCE_WORK_PER_SPAN * UTILIZATION  # MAC-eq of replay a span
        self.queue: list[list] = []  # [accepted_span, model, claims]
        self.issuance = []
        self.floor_blocks = []
        self.total_blocks_epoch = 0.0
        self.model_blocks_epoch = 0.0
        self.floor_epoch_blocks = 0.0
        self.competing = {}
        self.events = []

    # ---- ADR-0039 D5 epoch budgets, renormalised over the classes that produced in the closed epoch ----
    def budget_of(self, m, span: int) -> float:
        share = (1.0 - sum(k.share for k in self.models if k.registered_at <= span)) if m is None else m.share
        key = "floor" if m is None else m.name
        census = sum(self.competing.values())
        denom = census if key in self.competing else census + share
        if census == 0:
            denom = 1.0
        if denom <= 0:
            denom = 1.0
        return max(1.0, EPOCH_BLOCKS * share / denom)

    # ---- capacity ----
    def cap_window(self, m: Model) -> float:
        """Claims of m the panel could hold in m's window if it did nothing else."""
        return self.panel_per_span * m.window / (m.ccu * PANEL_SEATS)

    # ---- lifecycle (palw_lifecycle_step_v1) ----
    def lifecycle_step(self, m: Model, span: int, panel_ok: bool) -> None:
        util = m.inflight / max(self.cap_window(m), 1e-9)
        overloaded = util >= 1.0
        ready_enough = panel_ok and self.ready_seats >= PANEL_SEATS + 2
        span_stable = not overloaded and ready_enough
        s = m.state
        if s == "Registered":
            n = "Prefetching"
        elif s == "Prefetching":
            n = "Probation" if ready_enough else s
        elif s == "Probation":
            if not panel_ok or overloaded:
                n = "Held"
            elif m.probes >= PROBATION_CLAIMS and ready_enough:
                n = "ActiveLimited"
            else:
                n = s
        elif s == "ActiveLimited":
            if not panel_ok or overloaded:
                n = "Held"
            elif span_stable:
                m.stable += 1
                n = "Active" if m.stable >= STABLE_SPANS else s
            else:
                m.stable = 0
                n = s
        elif s == "Active":
            n = "Held" if (not panel_ok or overloaded) else s
        else:
            n = "Probation" if (ready_enough and not overloaded) else s
        if n != s:
            m.since, m.probes, m.stable = span, 0.0, 0
            self.events.append((span, f"{m.name} {s}->{n}"))
        m.state = n

    def registry_boundary(self, span: int, panel_ok: dict) -> None:
        rowed = [m for m in self.models if m.registered_at <= span]
        for m in rowed:
            self.lifecycle_step(m, span, panel_ok.get(m.name, True))
        if self.design in ("A0", "A1"):
            adm = {m.name: m.admission_per_span * ADMISSION_PERMILLE.get(m.state, 0) / 1000.0 for m in rowed}
            total = sum(adm.values())
            room = (1000 - FLOOR_MIN_PERMILLE) / 1000.0
            for m in rowed:
                m.share = room * adm[m.name] / total if total > 0 else 0.0
                if self.design == "A1" and not m.priced and m.share > 0 and m.state not in ("Registered", "Prefetching"):
                    m.p = min(1.0, m.share * 1000 * m.pwu / UNIT_SHARE_PWU)
                    m.priced = True
                    self.events.append((span, f"{m.name} seated p={m.p:.3e}"))
                elif self.design == "A0" and m.p == 0.0 and m.state != "Registered":
                    m.p = self.p_floor
                    self.events.append((span, f"{m.name} registered at the floor's price p={m.p:.3e}"))

    def epoch_boundary(self, span: int) -> None:
        total = self.total_blocks_epoch
        if self.design in ("A0", "A1"):
            # integer semantics: a class with fewer than half a block in the epoch was silent
            producing = [m for m in self.models if m.epoch_blocks >= 0.5 and m.p > 0]
            hardest = min([m.p for m in producing] + [self.p_floor])
            for m in self.models:
                if m.registered_at > span or m.p <= 0 or m.share <= 0:
                    continue
                expected = round(m.share * total)
                if expected == 0:
                    continue
                observed = round(m.epoch_blocks)
                if observed == 0:
                    if m.p < hardest:
                        m.p = min(m.p * MAX_FACTOR, hardest)
                else:
                    m.p = min(1.0, max(m.p / MAX_FACTOR, min(m.p * MAX_FACTOR, m.p * expected / observed)))
            # the floor's class DAA holds its residual share
            floor_share = 1.0 - sum(m.share for m in self.models if m.registered_at <= span)
            expected = round(floor_share * total)
            observed = round(self.floor_epoch_blocks)
            if expected > 0 and observed > 0:
                self.p_floor = min(1.0, max(self.p_floor / MAX_FACTOR, min(self.p_floor * MAX_FACTOR, self.p_floor * expected / observed)))
            elif observed == 0:
                self.p_floor = min(1.0, self.p_floor * MAX_FACTOR)
            if total > 0:
                self.p_net = min(1.0, max(self.p_net / MAX_FACTOR, min(self.p_net * MAX_FACTOR, self.p_net * EPOCH_BLOCKS / total)))
        else:
            observed = total if self.design == "B" else self.model_blocks_epoch
            if observed > 0:
                self.W = max(W0, self.W * max(1 / MAX_FACTOR, min(MAX_FACTOR, observed / EPOCH_BLOCKS)))
            else:
                self.W = max(W0, self.W / MAX_FACTOR)
        self.competing = {m.name: m.share for m in self.models if m.epoch_blocks >= 0.5}
        if self.floor_epoch_blocks >= 0.5:
            self.competing["floor"] = 1.0 - sum(m.share for m in self.models if m.registered_at <= span)
        for m in self.models:
            m.budget_used = 0.0
            m.epoch_blocks = 0.0
        self.total_blocks_epoch = self.model_blocks_epoch = self.floor_epoch_blocks = 0.0

    # ---- one span ----
    def step(self, span: int, supply_scale: dict, panel_ok: dict, rate: float) -> None:
        if span % EPOCH_SPANS == 0 and span > 0:
            self.epoch_boundary(span)
        self.registry_boundary(span, panel_ok)
        design = self.design
        model_blocks = 0.0
        per_model_attempted = {}
        load = sum(k.inflight * k.ccu * PANEL_SEATS for k in self.models)
        for m in self.models:
            if m.registered_at > span:
                for k in ("msk", "ccu", "final_ccu", "blocks", "claims", "p", "finals"):
                    m.hist[k].append(0.0)
                m.hist["state"].append("-")
                continue
            draws = m.supply * supply_scale.get(m.name, 1.0) * SPAN_S
            admitted = m.state in ("Probation", "ActiveLimited", "Active")
            p_net = self.p_net if self.p_net_draw else 1.0
            if design in ("A0", "A1"):
                p = m.p if admitted else 0.0
                budget = self.budget_of(m, span)
                pos = (span % EPOCH_SPANS) / EPOCH_SPANS
                others = sum(k.budget_used for k in self.models if k is not m) + self.floor_epoch_blocks
                release = max(0.0, pos * (EPOCH_BLOCKS - budget) - others)
                room = max(0.0, budget - m.budget_used) + release
                attempted = m.ccu / max(p * p_net, 1e-30) if p > 0 else 0.0
            else:
                # the verification budget: the panel's replay over m's window, less what every class
                # already holds in flight — one network-wide number, not a per-class share
                cost = m.ccu * PANEL_SEATS
                cap = max(0.0, (self.panel_per_span * m.window - load) / cost) + m.inflight
                if m.state == "Probation":
                    cap = min(cap, 1.0)
                headroom = max(0.0, 1.0 - m.inflight / max(cap, 1e-9)) if design == "E" else 1.0
                p = min(1.0, m.ccu / self.W) * headroom if admitted else 0.0
                room = max(0.0, cap - m.inflight) if design != "E" else float("inf")
                attempted = m.ccu / max(min(1.0, m.ccu / self.W), 1e-30)
            if design not in ("A0", "A1"):
                m.p = p
            if not admitted or p <= 0:
                accepted, spent = 0.0, (0.0 if self.hold else draws)
            else:
                wins = draws * p * p_net
                accepted = min(wins, room)
                # the producer's pre-check: it stops drawing once its class has no room
                spent = draws if not self.hold else min(draws, accepted / (p * p_net) if wins > 0 else 0.0)
            per_model_attempted[m.name] = attempted
            if accepted > 0:
                self.queue.append([span, m, accepted])
                m.inflight += accepted
                load += accepted * m.ccu * PANEL_SEATS
            m.budget_used += accepted
            m.epoch_blocks += accepted
            model_blocks += accepted
            m.hist["ccu"].append(spent * m.ccu)
            m.hist["blocks"].append(accepted)
            m.hist["claims"].append(accepted)
            m.hist["p"].append(p)
            m.hist["state"].append(m.state)
            m.hist["msk"].append(0.0)
            m.hist["final_ccu"].append(0.0)
            m.hist["finals"].append(0.0)
        # the shared panel drains the queue, oldest first; a claim past its window voids
        capacity = self.panel_per_span
        kept = []
        for item in self.queue:
            accepted_span, m, claims = item
            if not panel_ok.get(m.name, True):
                if span - accepted_span >= m.window:
                    m.inflight -= claims
                else:
                    kept.append(item)
                continue
            cost = m.ccu * PANEL_SEATS
            can = min(claims, capacity / cost) if cost > 0 else claims
            if can > 0:
                capacity -= can * cost
                claims -= can
                m.inflight -= can
                if design == "A1":
                    reward = min(ESCROW, per_model_attempted.get(m.name, m.ccu) / 1e9 * rate)
                else:
                    reward = ESCROW
                m.hist["msk"][-1] += can * reward * 0.8
                m.hist["finals"][-1] += can
                m.hist["final_ccu"][-1] += can * per_model_attempted.get(m.name, m.ccu)
                if m.state == "Probation":
                    m.probes += can
            if claims > 1e-12:
                if span - accepted_span >= m.window:
                    m.inflight -= claims  # void
                else:
                    kept.append([accepted_span, m, claims])
        self.queue = kept
        # the floor
        if design in ("A0", "A1"):
            budget = self.budget_of(None, span)
            pos = (span % EPOCH_SPANS) / EPOCH_SPANS
            others = sum(k.budget_used for k in self.models)
            release = max(0.0, pos * (EPOCH_BLOCKS - budget) - others)
            wins = self.floor_supply * SPAN_S * self.p_floor * self.p_net
            floor_blocks = min(wins, max(0.0, budget - self.floor_epoch_blocks) + release)
        elif design == "B":
            floor_blocks = self.floor_supply * SPAN_S * min(1.0, 30.5e6 / self.W)
        else:
            floor_blocks = max(0.0, BLOCKS_PER_SPAN - model_blocks)
        self.floor_blocks.append(floor_blocks)
        self.floor_epoch_blocks += floor_blocks
        self.total_blocks_epoch += model_blocks + floor_blocks
        self.model_blocks_epoch += model_blocks
        if span % EPOCH_SPANS == EPOCH_SPANS - 1:
            finals = sum(sum(m.hist["msk"][-EPOCH_SPANS:]) / 0.8 for m in self.models)
            floor_paid = sum(self.floor_blocks[-EPOCH_SPANS:]) * ESCROW if design in ("A0", "A1") else 0.0
            self.issuance.append(finals + floor_paid)

    def run(self, scenario) -> dict:
        for span in range(self.spans):
            supply_scale, panel_ok, rate = scenario(span)
            self.step(span, supply_scale, panel_ok, rate)
        return self.report()

    def report(self) -> dict:
        last = EPOCH_SPANS * 10
        out = {"design": self.design, "W": self.W, "events": self.events}
        rows = []
        total_final_ccu = sum(sum(m.hist["final_ccu"][-last:]) for m in self.models)
        total_blocks = sum(sum(m.hist["blocks"][-last:]) for m in self.models) + sum(self.floor_blocks[-last:])
        rhos = []
        for m in self.models:
            msk = sum(m.hist["msk"][-last:])
            ccu = sum(m.hist["ccu"][-last:])
            rho = msk / ccu * 1e9 if ccu > 0 else float("nan")
            rhos.append(rho)
            blocks = sum(m.hist["blocks"][-last:])
            per_epoch = [sum(m.hist["blocks"][i : i + EPOCH_SPANS]) for i in range(len(m.hist["blocks"]) - last, len(m.hist["blocks"]), EPOCH_SPANS)]
            mean = sum(per_epoch) / len(per_epoch) if per_epoch else 0.0
            cv = (math.sqrt(sum((x - mean) ** 2 for x in per_epoch) / len(per_epoch)) / mean) if mean > 0 else 0.0
            states = m.hist["state"][-last:]
            rows.append({
                "model": m.name, "kind": m.kind, "state": m.state, "rho": rho,
                "block_share": blocks / total_blocks if total_blocks > 0 else 0.0,
                "final_ccu_share": sum(m.hist["final_ccu"][-last:]) / total_final_ccu if total_final_ccu > 0 else 0.0,
                "claims_per_span": blocks / last, "finals_per_span": sum(m.hist["finals"][-last:]) / last,
                "p": m.p, "cv": cv, "held": sum(1 for s in states if s == "Held") / max(len(states), 1),
                "util": ccu / max(m.supply * SPAN_S * last * m.ccu, 1e-30),
            })
        finite = [r for r in rhos if r == r and r > 0]
        out["rho_spread"] = (max(finite) / min(finite)) if len(finite) > 1 else 1.0
        out["rows"] = rows
        out["issuance_per_epoch"] = sum(self.issuance[-10:]) / max(len(self.issuance[-10:]), 1)
        out["floor_block_share"] = sum(self.floor_blocks[-last:]) / total_blocks if total_blocks > 0 else 0.0
        out["blocks_per_span"] = total_blocks / last
        conv = 0
        for m in self.models:
            per_epoch = []
            for i in range(0, len(m.hist["msk"]) - EPOCH_SPANS + 1, EPOCH_SPANS):
                msk = sum(m.hist["msk"][i : i + EPOCH_SPANS])
                ccu = sum(m.hist["ccu"][i : i + EPOCH_SPANS])
                per_epoch.append(msk / ccu if ccu > 0 else float("nan"))
            final = [x for x in per_epoch[-10:] if x == x]
            if not final:
                continue
            target = sum(final) / len(final)
            if target <= 0:
                continue
            settled = 0
            for i in range(len(per_epoch) - 1, -1, -1):
                x = per_epoch[i]
                if x != x or abs(x - target) > 0.1 * target:
                    settled = i + 1
                    break
            conv = max(conv, settled)
        out["convergence_epochs"] = conv
        return out


# ---- fleets ----
PANEL_CCU_PER_S = READY_SEATS * 4e9 * UTILIZATION / PANEL_SEATS  # claim compute the panel can verify a second


def fleet(fractions: dict, supply_x: float, at: dict | None = None, split: dict | None = None) -> list[Model]:
    """Models whose forwards per second put `supply_x` x the panel's verifiable compute on the network,
    split by `fractions`; `split[name] = n` registers that model as n identical classes."""
    at = at or {}
    split = split or {}
    total = supply_x * PANEL_CCU_PER_S
    models = []
    for name, f in fractions.items():
        kind = name if name in CCU else "S"
        n = split.get(name, 1)
        for i in range(n):
            models.append(Model(name if n == 1 else f"{name}#{i}", kind, total * f / CCU[kind] / n, at.get(name, 0)))
    return models


def steady(**scale):
    def sc(span):
        return (dict(scale), {}, RATE_MAX_MSK_PER_G)
    return sc


SCENARIOS = {}


def scenario(name):
    def deco(fn):
        SCENARIOS[name] = fn
        return fn
    return deco


@scenario("Q25=100%")
def s1(x):
    return fleet({"Q25": 1.0, "Q36": 0.0}, x), steady()


@scenario("Q25/Q36=50/50")
def s2(x):
    return fleet({"Q25": 0.5, "Q36": 0.5}, x), steady()


@scenario("Q25/Q36=10/90")
def s3(x):
    return fleet({"Q25": 0.1, "Q36": 0.9}, x), steady()


@scenario("Q25/Q36/Kimi=20/30/50")
def s4(x):
    return fleet({"Q25": 0.2, "Q36": 0.3, "Kimi": 0.5}, x), steady()


@scenario("new model (Kimi) at epoch 60")
def s5(x):
    return fleet({"Q25": 0.5, "Q36": 0.3, "Kimi": 0.2}, x, at={"Kimi": 60 * EPOCH_SPANS}), steady()


@scenario("Q36 as 100 classes, same compute")
def s6(x):
    return fleet({"Q25": 0.5, "Q36": 0.5}, x, split={"Q36": 100}), steady()


@scenario("attacker: 1000 tiny classes, compute = Q25's")
def s7(x):
    return fleet({"Q25": 0.4, "Q36": 0.2, "S": 0.4}, x, split={"S": 1000}), steady()


@scenario("attacker: same compute as 1 tiny class")
def s7b(x):
    return fleet({"Q25": 0.4, "Q36": 0.2, "S": 0.4}, x), steady()


@scenario("Q36 supply x10 at epoch 60")
def s8(x):
    def sc(span):
        return ({"Q36": 10.0 if span >= 60 * EPOCH_SPANS else 1.0}, {}, RATE_MAX_MSK_PER_G)
    return fleet({"Q25": 0.5, "Q36": 0.5}, x), sc


@scenario("Q25 panel outage epochs 60-70")
def s9(x):
    def sc(span):
        e = span // EPOCH_SPANS
        return ({}, {"Q25": not (60 <= e < 70)}, RATE_MAX_MSK_PER_G)
    return fleet({"Q25": 0.5, "Q36": 0.5}, x), sc


@scenario("supply x0.1 at epoch 60 (target shock)")
def s11(x):
    def sc(span):
        f = 0.1 if span >= 60 * EPOCH_SPANS else 1.0
        return ({"Q25": f, "Q36": f}, {}, RATE_MAX_MSK_PER_G)
    return fleet({"Q25": 0.5, "Q36": 0.5}, x), sc


def fmt(x, digits=3):
    if x != x:
        return "nan"
    if x == 0:
        return "0"
    if abs(x) >= 1e4 or abs(x) < 1e-2:
        return f"{x:.{digits}e}"
    return f"{x:.{digits}f}"


def aggregate(rows):
    """Collapse `name#i` rows into one `name#*` row."""
    out, agg = [], {}
    for r in rows:
        if "#" not in r["model"]:
            out.append(r)
            continue
        key = r["model"].split("#")[0] + "#*"
        a = agg.setdefault(key, {"model": key, "kind": r["kind"], "state": r["state"], "rho": [], "block_share": 0.0, "final_ccu_share": 0.0, "claims_per_span": 0.0, "finals_per_span": 0.0, "p": r["p"], "cv": r["cv"], "held": r["held"], "util": []})
        a["rho"].append(r["rho"])
        a["util"].append(r["util"])
        for k in ("block_share", "final_ccu_share", "claims_per_span", "finals_per_span"):
            a[k] += r[k]
    for a in agg.values():
        fin = [x for x in a["rho"] if x == x]
        a["rho"] = sum(fin) / len(fin) if fin else float("nan")
        a["util"] = sum(a["util"]) / len(a["util"])
        out.append(a)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--designs", default="A0,A1,D,E,B")
    ap.add_argument("--epochs", type=int, default=120)
    ap.add_argument("--supply-x", type=float, default=1.0, help="model compute supply as a multiple of what the panel can verify")
    ap.add_argument("--ready-seats", type=int, default=READY_SEATS)
    ap.add_argument("--no-hold", action="store_true")
    ap.add_argument("--scenario", default=None)
    ap.add_argument("--events", action="store_true")
    args = ap.parse_args()
    designs = args.designs.split(",")
    names = [args.scenario] if args.scenario else list(SCENARIOS)
    print(f"supply = {args.supply_x} x panel capacity ({PANEL_CCU_PER_S:.2e} MAC-eq/s verifiable with {args.ready_seats} seats); W0 = {W0:.3e}; escrow {ESCROW} MSK; epoch {EPOCH_BLOCKS:.0f} blocks")
    for name in names:
        print(f"\n### {name}")
        print("| design | model | state | MSK/G-CCU | ρ spread | block share | Final CCU share | claims/span | Finals/span | p | util | osc CV | held | floor blk | blk/span | issuance/epoch | conv |")
        print("|" + "---|" * 17)
        for d in designs:
            models, sc = SCENARIOS[name](args.supply_x)
            sim = Sim(d, models, args.epochs * EPOCH_SPANS, ready_seats=args.ready_seats, hold=not args.no_hold)
            r = sim.run(sc)
            for i, row in enumerate(aggregate(r["rows"])):
                first = i == 0
                print(
                    f"| {d if first else ''} | {row['model']} | {row['state']} | {fmt(row['rho'])} | {fmt(r['rho_spread'], 2) if first else ''} | {fmt(row['block_share'])} | {fmt(row['final_ccu_share'])} | {fmt(row['claims_per_span'])} | {fmt(row['finals_per_span'])} | {fmt(row['p'], 2)} | {fmt(row['util'], 2)} | {fmt(row['cv'], 2)} | {fmt(row['held'], 2)} | {fmt(r['floor_block_share'], 2) if first else ''} | {fmt(r['blocks_per_span'], 2) if first else ''} | {fmt(r['issuance_per_epoch'], 0) if first else ''} | {r['convergence_epochs'] if first else ''} |"
                )
            if args.events and r["events"]:
                print("    events: " + "; ".join(f"{s}: {t}" for s, t in r["events"][:12]))


if __name__ == "__main__":
    main()
