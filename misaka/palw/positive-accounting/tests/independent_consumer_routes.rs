//! Routing checks use a recording port, independently of gate acceptance logic.
//! They do not assert that native dispatchers are connected or activated.
use misaka_palw_positive_accounting::credit::consumers::CreditConsumers;
use misaka_palw_positive_contract::ports::{CreditGate, CreditUse};
use std::cell::Cell;

struct RecordingGate {
    allow: Cell<bool>,
    last: Cell<Option<(u64, CreditUse)>>,
}
impl CreditGate for RecordingGate {
    type RootedClaim = u64;
    type Authorization = CreditUse;
    type Denial = CreditUse;
    fn authorize(&self, claim: &u64, usage: CreditUse) -> Result<CreditUse, CreditUse> {
        self.last.set(Some((*claim, usage)));
        if self.allow.get() { Ok(usage) } else { Err(usage) }
    }
}
#[test]
fn each_consumer_forwards_the_same_claim_and_its_own_credit_use_on_every_read() {
    let gate = RecordingGate { allow: Cell::new(true), last: Cell::new(None) };
    let consumers = CreditConsumers::new(&gate);
    for allow in [true, false] {
        gate.allow.set(allow);
        let expected = |usage| if allow { Ok(usage) } else { Err(usage) };
        assert_eq!(consumers.inference_reward(&51), expected(CreditUse::Reward));
        assert_eq!(gate.last.get(), Some((51, CreditUse::Reward)));
        assert_eq!(consumers.worker_payment(&58), expected(CreditUse::WorkerPayment));
        assert_eq!(gate.last.get(), Some((58, CreditUse::WorkerPayment)));
        assert_eq!(consumers.work_receipt(&52), expected(CreditUse::WorkReceipt));
        assert_eq!(gate.last.get(), Some((52, CreditUse::WorkReceipt)));
        assert_eq!(consumers.dag_weight(&53, u128::MAX), if allow { u128::MAX } else { 0 });
        assert_eq!(gate.last.get(), Some((53, CreditUse::DagWeight)));
        assert_eq!(consumers.rule_e_capital(&54, 981), if allow { 981 } else { 0 });
        assert_eq!(gate.last.get(), Some((54, CreditUse::RuleE)));
        assert_eq!(consumers.settlement(&55), expected(CreditUse::Settlement));
        assert_eq!(gate.last.get(), Some((55, CreditUse::Settlement)));
        assert_eq!(consumers.prefix_reuse(&56), expected(CreditUse::PrefixReuse));
        assert_eq!(gate.last.get(), Some((56, CreditUse::PrefixReuse)));
        assert_eq!(consumers.dependent_claim(&57), expected(CreditUse::Dependency));
        assert_eq!(gate.last.get(), Some((57, CreditUse::Dependency)));
    }
}
