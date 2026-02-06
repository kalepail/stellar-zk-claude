# Application Verification Patterns

Reusable patterns for proving deterministic application behavior on Stellar.

## 1) Input-Only Claims
Use compact user-provided input traces (not direct state claims). Verifier
recomputes authoritative state transitions.

Benefits:
- Reduces attack surface for forged state.
- Makes proof statement easier to audit.
- Keeps replay deterministic and inspectable.

## 2) Canonical Transition Order
Document and freeze step order per frame/tick/transaction.

Rule:
- Equivalent logic with different order is not equivalent for proofs.

## 3) Deterministic Numeric Model
For consensus-critical logic, avoid platform-sensitive arithmetic.

Preferred:
- Integer/fixed-point formats
- Explicit overflow/wrap behavior
- Squared-distance comparisons instead of roots where possible

## 4) RNG Domain Separation
Separate authoritative randomness from cosmetic randomness.

Pattern:
- `gameplay_rng`: influences proven state
- `visual_rng`: non-consensus outputs only

## 5) Invariant Rule Groups
Define invariants by domain and emit stable error codes.

Common groups:
- Input format/parsing
- Global transition rules
- Entity/state rules
- Collision/interaction rules
- Progression/accounting rules

## 6) Final Commitment Checks
At end of replay/execution, compare final commitments against claims:
- terminal state hash
- score/value outputs
- RNG state / sequence commitment

These checks detect divergence even when it starts early and cascades.

## 7) Snapshot API for Auditable Verification
Expose read-only deterministic snapshots for rule checks and diagnostics.

Use snapshots to:
- detect first failing frame/step
- generate reproducible failure reports
- support differential testing between implementations

## 8) Cross-Implementation Parity
If you have multiple runtimes (for example TS + Rust):
- run same fixtures through both
- compare step-by-step state fields
- gate releases on parity

## 9) Versioned Verification
Treat verification logic as versioned protocol:
- version tape/trace format
- version program/verifier IDs
- migration plan for rule changes

## 10) Production Controls
- Disable dev/mock proofs in production paths.
- Enforce payload limits and strict parsing.
- Keep observability around rejection codes and latency.
