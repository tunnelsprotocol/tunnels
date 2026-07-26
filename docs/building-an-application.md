# Building an Application on Tunnels

The protocol records contributions and routes value. Everything people actually
touch — projects, tasks, teams, approvals — is yours to build. This document is
the minimum surface an application must model, learned from the first production
integration. If your application can express these ten things, it can sit on
Tunnels; if it skips one, a specific failure follows, noted in each section.

## 1. A project container
One application project ↔ one on-chain project. Register it once
(`RegisterProject`) with immutable parameters — ρ reserve, φ fee, γ genesis,
δ predecessor, verification window, bonds — and store the returned 20-byte
chain id. Set `evidence_hash` (e.g., a hash of your internal project id):
registrations with identical parameters from the same creator are otherwise
byte-identical and collide. *Skip this and your users' work has nowhere to go.*

## 2. An owner of record
Know who founded each project. The founder signs its registration (or your
platform identity registers on their behalf and holds γ for them — settlement
is application-layer either way), and gates whatever your top contribution
weight is. *Skip this and γ has no honest destination.*

## 3. Units of work with a weight vocabulary
Decide what a contribution is and what it weighs. Show words, record numbers
(e.g., Small/Medium/Large/Milestone → 1/3/8/20 units). The protocol sees only
units; legibility is your job. Weights must be set before work completes, not
negotiated after. *Skip this and the labor pool splits over noise.*

## 4. A completion and review flow
Attested work must first survive your own acceptance step. The shape that works:
work marked done enters a review window (24h+) where teammates can send it
back; silence is acceptance; settled work locks and becomes attestable. This
mirrors the protocol's own negative-option challenge model. *Skip this and your
challenge window fills with disputes your UI should have prevented.*

## 5. A silent per-user identity
Embed the SDK. Generate each user's key in their browser at first login; store
the address server-side, never the key; persist the key encrypted on-device;
allow multiple identities per account (one per device) and aggregate them in
your UI. Offer an encrypted backup download once there is something to protect.
*Skip the SDK and you become a custodian — the one thing the protocol exists
to prevent.*

## 6. A relay
Browsers cannot reach node RPC cross-origin. Run an authenticated endpoint that
forwards a whitelisted method set (`submitTransaction`, the read methods) to
your node. Never whitelist `mine`. *Skip this and client transactions have no
path to the chain.*

## 7. A sweep
On session start, fire-and-forget: attest the user's settled work (evidence =
hash of a canonical record string), then retry whatever failed last time. Never
block the UI on the chain; the application must work identically when the node
is unreachable. *Skip the fire-and-forget property and chain hiccups become
user-facing outages.*

## 8. Two bookkeeping tables
Identities (user ↔ address) and attestations (work item ↔ tx id, attestation
id, units, evidence hash, lifecycle timestamps). These mirror the chain for
fast queries and — more important — reconciliation: a script that diffs your
books against `get_attestations_by_project` is your proof of integrity. *Skip
this and you cannot demonstrate your app and the ledger agree.*

## 9. Your platform's own identity
Your application holds one key of its own (custody of your own key is correct —
it acts as itself, per the integration pattern): registering legacy projects,
finalizing expired attestations hourly (finalization is permissionless —
contributors should never need to return to collect), and signing adjudications
where it is a project's adjudicator.

## 10. Real money, when a project gets funded
The router is enforceable accounting, not a bank. It never custodies funds —
your application holds the actual dollars (escrow account, payment processor)
and settles them; the chain guarantees that whatever enters distributes
correctly and visibly. Get four things right before the first dollar:

**Sort every payment into one of three lanes.** Reversible commerce (vendor
orders, refundable purchases) stays on your payment processor — the router has
no refunds: deposits split instantly, deterministically, and permanently ("the
protocol will not take back what it owes you"). Earned compensation flows
through the router. Reputation-only records are attestations with zero deposit.
If a payment could ever need reversing, it does not belong in the router.

**Meter funding in tranches — never deposit a budget up front.** A deposit
splits over units attested *as of that moment*. Deposit $10M on day one and it
all flows to whoever has attested by day one. Hold funds in your escrow and
deposit draws as work settles; each tranche then pays the ledger as it actually
stands. The escrow is the tank; the router is the valve.

**Route only contribution compensation.** Production budgets are mostly costs —
vendors, rentals, locations. Pay those from escrow directly. Only the
compensation pool goes through the waterfall, or your contributors' pro-rata
math is polluted and your φ fee is charging on camera rentals.

**Close the loop honestly.** `DepositRevenue` is signed by the project's
deposit authority (typically your platform identity). Contributors claim with
`ClaimRevenue` — their own key, nobody else's — and your application settles
claimable balances into real-world payments. Two obligations at this stage:
show every participant their actual split *before* the first deposit (the
parameters were always public; now they mean money), and start posting real
attestation bonds as the application (Rules 11 and 14 — the contributor's cost
stays zero precisely when the stakes stop being zero).

## What the protocol will not do for you
Authentication, sessions, contribution categories, project access control,
discovery, notifications, dispute nuance beyond finalize/reject, fiat
settlement, and evidence storage are yours. The protocol provides primitives;
you provide policy. If you expect the ledger to handle any of these, the design
is wrong on your side of the boundary.
