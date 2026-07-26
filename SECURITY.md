# Security

## Security Model

The first question every blockchain-literate reader asks: *no mining rewards —
then what secures this?* The answer is that Tunnels is not secured the way
Bitcoin is, because it is not protecting the same thing.

Bitcoin's chain is the money itself; forging the chain steals the coins. So
Bitcoin makes writing history physically expensive — proof-of-work funded by
minted coins. The burn is the security, and inflation pays for it. Tunnels
deliberately outgrew that method: the burn costs money the protocol has no way
to pay (no token, no rewards, no fees — by rule), and the method creates
nothing here, because a Tunnels block is not money. It is a record about work
whose value lives outside the chain.

Tunnels' security therefore lives at the **transaction layer**, paid in
accountability rather than electricity:

- **Cryptography:** only a contributor's key can attest as them. Forgery does
  not yield to hash power at any scale.
- **Economics:** every attestation is backed by a bond and exposed to a
  challenge window. Inflated claims are cheap to dispute and expensive to lose.
- **Reputation:** identities are permanent and non-transferable. Every claim
  stakes a history that cannot be bought, sold, or reset.

Blocks retain one job: placing finished work in permanent order. Production is
event-driven — a transaction's acceptance triggers the block that carries it —
and per-transaction proof-of-work (small, self-adjusting, paid to no one)
throttles spam. That is the part of Bitcoin's method that still serves a
contribution ledger, and the only part retained.

**The trade, stated plainly:** Tunnels gives up brute-force resistance to
history *reordering*, which for Bitcoin would be fatal. Here, a reorderer gains
almost nothing: signatures cannot be forged, nothing can be minted, and every
honest node rejects invalid history on sight. The realistic attack on a record
of work is forgery — and forgery is impossible at any hash rate. Different
asset, different threat model, different — and appropriate — defense.


## Reporting a Vulnerability

If you discover a security vulnerability in the Tunnels Protocol, please report it by emailing:

**accounts@tunnelsprotocol.org**

Please include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Any suggested fixes (optional)

We will acknowledge receipt within 48 hours and provide a detailed response within 7 days.

## Scope

This policy covers:
- tunnels-node
- tunnels-core
- tunnels-sdk
- tunnels-wallet
- All other crates in this repository

## Responsible Disclosure

We ask that you:
- Give us reasonable time to address the issue before public disclosure
- Avoid accessing or modifying other users' data
- Act in good faith to avoid privacy violations and service disruption
