# Asteroids Docs

Canonical documentation for the Asteroids game, deterministic verification, and
ZK/Stellar integration.

## Canonical Sequence
| File | Purpose |
|---|---|
| `00-OVERVIEW.md` | One-page system map and decisions |
| `01-GAME-SPEC.md` | Gameplay rules and progression |
| `02-VERIFICATION-SPEC.md` | Deterministic verification and tape contract |
| `03-ZK-AND-STELLAR-ARCHITECTURE.md` | Proving flow and on-chain settlement model |
| `04-INTEGER-MATH-SPEC.md` | Fixed-point and deterministic arithmetic |
| `05-PROVING-SYSTEM-DECISION.md` | RISC Zero vs Noir decision and criteria |
| `06-IMPLEMENTATION-STATUS.md` | Current implementation state and gaps |
| `07-TESTING-AND-OPERATIONS.md` | Test strategy and operational defaults |
| `08-SOURCES.md` | Curated source list used to derive this spec |
| `09-SCORE-TOKEN-CONTRACT.md` | Soroban score-submission and token minting contract spec |
| `10-PROOF-GATEWAY-SPEC.md` | Cloudflare Worker + prover gateway behavior and API contract |
| `11-CLIENT-INTEGRATION-SPEC.md` | Frontend wallet/proof/claim integration contract |
| `12-GUEST-OPTIMIZATION.md` | RISC0 guest and proving optimization notes |
| `13-ORIGINAL-RULESET-VARIANCE-AUDIT.md` | Current-vs-original gameplay variance audit |
| `14-VARIANCE-RESOLUTION-PLAN.md` | Resolution strategy for selected variance items |
| `15-DOCS-PARITY-CHECKLIST.md` | Code-backed docs parity checklist (current session) |

## Scope
- These files are current truth.
- Historical drafts are intentionally removed.
- If a decision changes, update the canonical file directly.
