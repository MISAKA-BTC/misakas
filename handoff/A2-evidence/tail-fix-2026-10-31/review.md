# A0 protected-tail blocker correction

A0's finding is valid: a public checksum and syntactic delta invariants cannot authorize an unknown prepared-unit identity or cost. The vulnerable reader promoted such state after encountering a trusted prefix. The correction removes all tail adoption/CAS promotion.

Root implemented strict exact-prefix recovery and separate durable discard. Independent audit authored the first five forgery/lease/exact-prefix cases, including real 4-KiB source openings and a signed cap below the next actual unit's bytes. The former recovery implementer independently reviewed lease/anchor ordering and identified the uncertain-successful-CAS distinction. Root added that case, oversized-tail discard, and authenticated-size retry defense. Review found the new boundary consistent with the required protected-store assumptions.

The old syntactic validator is explicitly shown accepting the recomputed-checksum attack payloads; strict resume refuses them and repair restores only the real prefix. Raw logs print protected/forged unique meters and actual post-repair costs. A 5,000-byte cap with 4,096 bytes already spent refuses the next actual 4,096-byte unit before provider I/O. Valid protected reads, reservations and prepared entries are never discarded.

No native authority, cryptographic computation proof, public activation or new shared wire schema is introduced. Protected anchors promoted by the old vulnerable reader cannot be repaired into authenticity using this same untrusted file; protected-store provenance remains the owner's responsibility.
