# Tunnels Protocol: Implementation Rules

These rules govern every implementation decision for the Tunnels protocol. They are
not guidelines. They are constraints with the same force as the protocol's five
invariants. When a design choice is ambiguous, these rules resolve it. When a
dependency is proposed, these rules evaluate it. When a shortcut is available,
these rules determine whether it is acceptable.

These rules exist because the protocol's integrity depends not only on what it does
but on how it is built. A correct protocol deployed on compromised infrastructure is
a compromised protocol.

1. **Build from scratch whenever possible.** External platforms introduce external
   governance, upgrade mechanisms, fee structures, and points of failure. The
   protocol's invariants cannot be guaranteed by infrastructure it does not control.
2. **The protocol must be decentralized.** No single node, operator, organization,
   or jurisdiction can be a point of failure.
3. **The core code can never be modified.** No governance function, no proxy
   pattern, no admin key. If the protocol must evolve, a new version deploys as a
   separate instance; the predecessor/delta mechanism is the migration path.
4. **No native token.** The protocol moves value; it does not create value to move.
   The moment a token exists, speculation follows.
5. **No external dependencies that can be revoked.** If a third party can turn it
   off, it is not decentralized.
6. **Fully operational with zero infrastructure operated by its creators.** The
   Bitcoin test: Satoshi disappeared and it kept running.
7. **All protocol state must be derivable from the ledger alone.** Off-chain
   services may make queries faster. They must never be necessary.
8. **No permissioned operations.** The deploying identity has no ongoing capability
   any other participant lacks.
9. **Cryptographic choices must be conservative and interoperable.** Ed25519,
   SHA-256 — decades of scrutiny, not novel constructions.
10. **The protocol must function at minimum viable scale.** Two identities and one
    project work exactly the same as two million and a million.
11. **Transaction costs to the contributor must be zero.** Bonds are posted by the
    application or a third party and returned in full on finalization.
12. **No data the protocol does not need.** Hashes on-chain; the data itself never.
13. **No deployment on, dependency on, or integration with any existing
    blockchain.** Existing chains are prior art, not dependency.
14. **Contributors never pay to contribute. The protocol charges no fees.
    Applications bear the cost of infrastructure and bonds.** No fee mechanism, no
    block reward, no reward pool. Applications run nodes because that is the cost
    of operating a business on the protocol.
15. **The protocol ships the tools that make sovereign key management the
    default.** The easy path and the sovereign path are the same path.

*These rules are the protocol's conscience expressed as engineering constraints.
They do not make implementation easier. They make implementation honest.*
