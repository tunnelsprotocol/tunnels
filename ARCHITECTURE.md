# Architecture

Technical overview of the Tunnels protocol implementation.

## Single-Key Identity Model

Every participant has exactly one Ed25519 keypair. The on-chain address is derived deterministically from the public key via `derive_address()`. There is no key rotation, no multi-sig, no delegation. One key signs everything.

This is deliberate. Identities come in two types:

- **Human** — a persistent identity controlled directly by a person. Cannot be deactivated. Can create Agent children.
- **Agent** — linked to a parent Human identity. Revenue earned by an Agent routes automatically to its parent. Can be deactivated by the parent. Cannot create other Agents.

Keys are non-transferable because contribution history is permanently bound to the address derived from the public key. You cannot buy someone else's track record, and you cannot separate a reputation from the key that earned it. This prevents capital capture of attribution — no one can acquire influence by purchasing identities.

## Attestation State Machine

An attestation is a cryptographic record that work happened, signed by the contributor's key and backed by an economic bond.

```
                  ┌──────────────────────┐
                  │       Pending        │
                  └──────┬──────┬────────┘
                         │      │
          challenge filed │      │ verification window expires
                         │      │ (FinalizeAttestation)
                         ▼      ▼
              ┌───────────┐    ┌───────────┐
              │ Challenged │    │ Finalized │  ← terminal
              └─────┬──┬──┘    └───────────┘
                    │  │
   ruling: Valid ───┘  └─── ruling: Invalid
                    │            │
                    ▼            ▼
             ┌───────────┐  ┌──────────┐
             │ Finalized │  │ Rejected │  ← terminal
             └───────────┘  └──────────┘
```

**Pending**: Within the verification window, not yet challenged. Can transition to Challenged (if someone files a challenge) or Finalized (if the window expires without challenge).

**Challenged**: A challenge has been filed. The contributor can respond. An adjudicator resolves. Transitions to Finalized (ruling: valid) or Rejected (ruling: invalid).

**Finalized**: Terminal state. The attestation's units count toward distribution. Immutable.

**Rejected**: Terminal state. The attestation is invalidated. Its weight never enters distribution. Bond is forfeit.

Every attestation must survive a mandatory challenge window (minimum 24 hours) before finalization. This ensures that fraudulent contributions can always be disputed.

## Waterfall Revenue Router

Revenue deposited into a project flows through a deterministic waterfall. Each slice is computed from the remaining amount after prior slices, using basis-point arithmetic (1 bp = 0.01%):

```
Revenue Deposit
      │
      ▼
┌─────────────────────────────────┐
│ 1. Predecessor (delta%)         │  → recursive waterfall into predecessor project
│    remaining -= predecessor_amt │
├─────────────────────────────────┤
│ 2. Reserve (rho%)               │  → project reserve (claimable by project creator)
│    remaining -= reserve_amt     │
├─────────────────────────────────┤
│ 3. Fee (phi%)                   │  → fee recipient identity
│    remaining -= fee_amt         │
├─────────────────────────────────┤
│ 4. Genesis (gamma%)             │  → genesis recipient (identity or project)
│    remaining -= genesis_amt     │
├─────────────────────────────────┤
│ 5. Labor Pool (remainder)       │  → pro-rata to contributors by attested units
└─────────────────────────────────┘
```

The cascading remainder formula: each percentage is applied to what remains after the previous step, not to the original deposit. This means the effective percentages compound downward.

All distribution uses U256 fixed-point arithmetic (scaled by 2^128) to prevent rounding errors. The accumulator tracks a global `reward_per_unit` index that increments with each deposit, enabling O(1) distribution to any number of contributors without iterating over them.

## Predecessor Chain

A project can declare a predecessor at creation time. This creates an immutable revenue link: a percentage (delta) of all revenue deposited into the child project flows through the waterfall of the predecessor.

Predecessors chain recursively — if project C declares B as predecessor, and B declares A, then revenue into C flows partially to B, which flows partially to A. This enables derivative works to compensate original creators automatically.

The chain has a maximum depth of 8 to prevent infinite recursion. At depth > 8, remaining funds are credited to unallocated. The predecessor link and delta percentage are immutable after project creation.

## The Five Invariants

These are the protocol's hard guarantees, each enforced by the implementation and verified by dedicated test suites:

**1. Non-Transferability**
No transaction sequence can move contribution units from identity A to identity B. Units are permanently bound to the contributor's address. You cannot buy, sell, or transfer attribution.

**2. Immutable Attestations**
Once an attestation reaches a terminal state (Finalized or Rejected), it cannot be modified or removed. Contribution history is append-only. There are no transitions out of terminal states.

**3. Deterministic Distribution**
Given identical inputs (same transactions, same order, same timestamps), the state machine produces identical outputs. Same root hash, same balances, same reward indices. No randomness, no subjective decisions, no discretion.

**4. Transparent Parameters**
All project distribution parameters (rho, phi, gamma, delta, verification window, bond requirements) are set at creation, stored on-chain, and queryable by anyone. They never change after creation.

**5. Auditable Ledger**
Every transaction is recorded on-chain. All state changes are deterministic from the transaction history. The ledger is append-only — no transaction removal, no state rewriting. Any observer can replay the chain and verify every balance.

These five invariants reinforce each other: non-transferability prevents gaming the distribution; immutable attestations ensure the inputs to distribution never change retroactively; deterministic distribution means transparent parameters produce predictable outcomes; and the auditable ledger lets anyone verify that all invariants hold.

## Transaction Flow

How a transaction moves through the system, from wallet to network:

```
┌────────────┐     ┌────────────────┐     ┌──────────────┐
│   Wallet   │────▶│  JSON-RPC API  │────▶│   Mempool    │
│            │     │  (tunnels-node │     │ (tunnels-    │
│ Sign tx    │     │   /rpc)        │     │  chain)      │
│ with Ed25519     │                │     │              │
└────────────┘     │ Endpoint:      │     │ Validate:    │
                   │ transactions   │     │ - signature  │
                   │ _submit        │     │ - format     │
                   └────────────────┘     └──────┬───────┘
                                                 │
                                                 ▼
┌────────────┐     ┌────────────────┐     ┌──────────────┐
│  P2P       │◀────│  Block         │◀────│    State     │
│  Network   │     │  Production    │     │  Execution   │
│            │     │  (tunnels-     │     │ (tunnels-    │
│ Gossipsub  │     │   chain)       │     │  state)      │
│ broadcast  │     │                │     │              │
│ to peers   │     │ Assemble block │     │ Apply tx to  │
│            │     │ compute state  │     │ state, update│
│            │     │ root hash      │     │ balances     │
└────────────┘     └────────────────┘     └──────────────┘
```

1. **Wallet** creates and signs a transaction with the user's Ed25519 private key
2. **JSON-RPC API** receives the signed transaction via `transactions_submit`
3. **Mempool** validates the signature and queues the transaction
4. **State Execution** applies the transaction: dispatches to the appropriate handler (one of 13 transaction types), updates identities/projects/attestations/balances
5. **Block Production** assembles pending transactions into a block, executes them against state, computes the Merkle state root
6. **P2P Network** gossips the new block to all connected peers via libp2p/gossipsub

The 13 transaction types: CreateIdentity, CreateAgent, UpdateProfileHash, DeactivateAgent, RegisterProject, SubmitAttestation, ChallengeAttestation, RespondToChallenge, ResolveChallenge, FinalizeAttestation, DepositRevenue, ClaimRevenue, ClaimReserve.

## Storage

`tunnels-storage` persists all state to RocksDB with Merkle Patricia Trie commitments. Each block header contains a `state_root` hash — a cryptographic commitment to the entire state after applying all transactions in that block. This allows any node to verify state consistency and enables light client verification.

## P2P Networking

`tunnels-p2p` handles peer discovery, connection management, and data propagation using libp2p. Block and transaction gossip uses the gossipsub protocol. The sync module handles initial block download and catching up after disconnection. An LRU peer cache and DNS-based discovery help maintain network connectivity.
