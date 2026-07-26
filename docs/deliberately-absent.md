# Deliberately Absent

Every item on this list looks like a missing feature. None of them are. Each is a
decision, made deliberately, documented here so that no future contributor —
human or AI — "completes" the protocol into something it refuses to be. Before
adding anything to Tunnels, check this list. If it's here, the answer is no, and
the reason is one sentence away.

- **No native token.** The protocol moves value; it does not create value to move.
  Anything mintable becomes a speculative asset (Rule 4).
- **No block rewards and no protocol fees.** There is nothing to pay miners with,
  by design. Node operation is a business cost of the applications that profit
  from the network (Rule 14).
- **No continuous or scheduled mining.** Block production is event-driven:
  a transaction's acceptance triggers the block that packages it. Blocks are
  notarization of work, not a lottery. A node with an empty mempool produces
  nothing, forever. Do not add a heartbeat, a block interval, or empty blocks.
- **No public `mine` RPC outside devnet.** Public networks produce blocks through
  the node's own event-driven path only.
- **No key rotation and no key recovery.** Every recovery door is also a seizure
  door. Loss is permanent by design; applications handle continuity by linking a
  member's new identity, never by holding keys.
- **No delegated signing.** No identity can sign on behalf of another —
  cryptographically impossible, not merely discouraged. The platform holding
  zero member keys is the product, not a limitation.
- **No admin keys, no governance, no upgradeability.** Deployed core logic is
  immutable. Evolution = new deployment + predecessor linking (Rule 3).
- **No partial challenge outcomes.** Finalized or rejected. Nuance lives at the
  application layer; the protocol records the binary result.
- **No time-varying splits or recoupment breakpoints.** Waterfall percentages are
  fixed for the life of a project. "Recoup then flip" is not expressible, and that
  is intentional; the predecessor mechanism is the protocol-native investor model.
- **No on-chain names, profiles, or evidence content.** Hashes only (Rule 12).
  Work is transparent; identity is anonymous by default.
- **No bridges, no EVM, no other chains** (Rule 13).

## Consequences to respect (not absences — behaviors)

- **Transactions are deterministic.** Ed25519 is deterministic: identical fields
  from the same key produce byte-identical transactions and identical ids. Two
  projects registered with identical parameters by the same creator collide.
  The `evidence_hash` field is the distinguishing input — always set it.
- **Consensus assigns timestamps.** Applications cannot backdate anything; a
  catch-up entry is visibly a catch-up entry. This is a feature.
- **Verification windows have a 24-hour protocol minimum.** Applications may
  choose longer. Never shorter, and no mechanism exists to shorten one.
