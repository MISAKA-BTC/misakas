# PALW key and historical bond utility

The DNS validator daemon and DNS bond onboarding have been removed under RFC-0012.
`run`, `bond`, `equivocate`, signed-epoch databases and precommit scheduling are no
longer supported. For PALW panel participation, run `misaka verifier setup`.

The binary retains these commands:

* `keygen` — create an ML-DSA-87 key and its funding address;
* `status` — inspect a historical bond;
* `unbond` — submit the owner's authorized historical bond exit request;
* `balance` — inspect wallet balances;
* `spam` — generate native wallet transactions for a local drill.

Use `kaspa-pq-validator <command> --help` for key, network and RPC options.
Keys remain compatible with PALW and wallet signing. Existing bond exits still obey
the bond's consensus spend lock and unbonding period.
