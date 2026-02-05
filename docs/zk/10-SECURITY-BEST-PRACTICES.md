# Security Best Practices for ZK on Stellar

## Sources

- **Veridise**: https://veridise.com/blog/audit-insights/building-on-stellar-soroban-grab-this-security-checklist-to-avoid-vulnerabilities/
- **OpenZeppelin**: https://www.openzeppelin.com/news/developer-guide-to-building-safe-noir-circuits

---

## Soroban Smart Contract Security

### Vector and Map Type Safety

Soroban uses a `Val` representation internally. Improper type conversions between `Val` and expected Rust types can introduce vulnerabilities.

**Do:**
```rust
// Explicit type conversion with error handling
let value: i128 = env.storage().get(&key)
    .unwrap_or_else(|| panic_with_error!(&env, Error::NotFound));
```

**Don't:**
```rust
// Unsafe unwrap without error context
let value = env.storage().get(&key).unwrap();
```

### Fuzz Testing

Stellar supports fuzzing for Soroban contracts. Key practices:

1. **Use `panic_with_error!`** instead of bare `panic!` - allows fuzzers to distinguish between legitimate errors and actual bugs
2. **Implement property-based tests** to find edge cases in ZK verification logic
3. **Test with malformed proofs** to ensure verifier properly rejects invalid inputs

### Dependency Management

- **Pin exact versions** of all cryptographic dependencies
- Outdated contract dependencies can introduce vulnerabilities
- Use `cargo audit` to check for known vulnerabilities
- Verify that `ark-bls12-381` and `ark-bn254` crate versions match what's used in the host

### Unbounded Data Prevention

- Limit Merkle tree depth to prevent storage exhaustion
- Set maximum deposit/withdrawal amounts
- Implement rate limiting for ZK proof submissions
- Monitor storage growth for privacy pool contracts

---

## ZK Circuit Security (Circom)

### Soundness Issues

**Problem**: Circuits that accept invalid proofs
- Always verify that all constraints are satisfied
- Test with deliberately invalid inputs
- Ensure nullifier binding is correct

### Completeness Issues

**Problem**: Circuits that reject valid proofs
- Test full lifecycle: deposit -> prove -> withdraw
- Verify Merkle tree reconstruction matches on-chain state
- Check field element representations match between tools

### Input Validation

```circom
// Always validate public inputs
template SafeWithdraw() {
    signal input nullifierHash;
    signal input root;
    signal input recipient; // IMPORTANT: Prevents frontrunning

    // Verify nullifier is properly bound
    component hasher = Poseidon(1);
    hasher.inputs[0] <== nullifier;
    nullifierHash === hasher.out;
}
```

### Frontrunning Protection

The Privacy Pools implementation notes this as future work:
- Include recipient address in the ZK proof
- Without this, a relayer can redirect withdrawals to arbitrary addresses
- Critical for production deployments

---

## Noir Circuit Security (OpenZeppelin Guide)

### Arithmetic in Finite Fields

**Overflow/Underflow**: Noir operates in finite fields. Overflow wraps around silently.

```noir
// DANGEROUS: Overflow wraps in finite fields
fn dangerous(x: Field) -> Field {
    x + 1  // If x is the max field element, this wraps to 0
}

// SAFE: Add range checks where needed
fn safe(x: Field) {
    // Ensure x is within expected range
    assert(x as u64 < 1000000);
}
```

### Missing Range Checks

Unintended values can propagate through circuits:

```noir
// DANGEROUS: No range check
fn check_age(age: Field) -> bool {
    age > 18  // age could be negative in the field
}

// SAFE: Constrain the range
fn check_age_safe(age: u32) -> bool {
    age > 18 && age < 200
}
```

### Intent vs. Implementation Mismatches

Circuits that prove knowledge of secrets but fail to bind identifiers:

```noir
// DANGEROUS: Proves knowledge of secret but not binding to user
fn prove_knowledge(secret: Field, hash: pub Field) {
    assert(std::hash::poseidon2::Poseidon2::hash([secret], 1) == hash);
}

// SAFE: Binds proof to specific user
fn prove_ownership(secret: Field, user_id: pub Field, hash: pub Field) {
    assert(std::hash::poseidon2::Poseidon2::hash([secret, user_id], 2) == hash);
}
```

### Privacy Leaks

Even with ZK proofs, information can leak:
- **Public inputs**: Minimize what's public
- **Timing attacks**: Proof generation time can reveal input size
- **Output correlation**: If public outputs correlate with private inputs

---

## Operational Security

### Deployment Checklist

- [ ] All ZK circuits audited by qualified firm
- [ ] Trusted setup ceremony properly conducted (Groth16)
- [ ] Verification keys hardcoded (not admin-updateable) or secured
- [ ] Nullifier storage properly indexed for O(1) double-spend checks
- [ ] Merkle tree depth appropriate for expected usage
- [ ] Gas/instruction budget tested under worst-case conditions
- [ ] Proof format validation before deserialization
- [ ] Error handling doesn't leak information about private inputs
- [ ] Frontrunning protection implemented (recipient in proof)
- [ ] Rate limiting on proof submissions
- [ ] Monitoring for unusual patterns

### Trusted Setup (Groth16)

If using Groth16:
- Use community Powers of Tau ceremony results when available
- Never use a setup where you control all randomness
- Document the ceremony participants and process
- Consider switching to a setup-free system (RISC Zero STARKs) for higher security

### Key Management

- Verification keys: Immutable once deployed, or require multi-sig governance
- Prover keys: Keep off-chain, never expose to users
- User secrets: Generate client-side, never transmit to backend

---

## Common Vulnerabilities in ZK Applications

| Vulnerability | Impact | Mitigation |
|--------------|--------|------------|
| Double-spend | Loss of funds | Nullifier tracking with proper uniqueness checks |
| Frontrunning | Redirected withdrawals | Include recipient in ZK proof |
| Malformed proofs | False verification | Strict input validation before deserialization |
| Merkle tree manipulation | False inclusion proofs | On-chain Merkle root integrity |
| Trusted setup compromise | All proofs forgeable | Multi-party ceremony, or use STARKs |
| Circuit underconstrained | Accept invalid proofs | Thorough circuit audit, fuzzing |
| Side-channel leaks | Privacy loss | Constant-time operations, minimize public data |

---

## Formal Verification Advances

### R0VM 2.0 Formal Verification
Nethermind and Veridise (using the Picus tool) formally verified RISC Zero's R0VM 2.0 — the first zkVM to achieve formal verification. This is significant for Stellar because:
- RISC Zero proofs settle on Stellar via the Boundless marketplace
- Formal verification proves the verifier itself is correct (not just the proofs)
- Blog: https://www.nethermind.io/blog/we-verified-the-verifier-a-first-for-zero-knowledge-proof-systems

### Certora Continuous Integration
Certora provides ongoing formal verification for Soroban contracts (not just one-time audits):
- First WASM-powered platform to support Certora's formal verification
- Automated property checking runs on every contract update
- Has secured ~$25B in Ethereum protocols, now available for Stellar
- Supports ZK verifier contract verification

### OpenZeppelin Soroban Security Detector SDK
Automated vulnerability detection tool specifically for Soroban:
- Static analysis for common vulnerability patterns
- ZK-specific checks for constraint issues and under-constrained circuits
- Integrates into CI/CD pipelines for continuous security

---

## Audit Resources

| Firm | Expertise | URL |
|------|-----------|-----|
| Veridise | Soroban core audits, R0VM formal verification, Picus tool | https://veridise.com |
| Certora | Formal verification (first WASM support), continuous integration | https://certora.com |
| OpenZeppelin | Smart contract & Noir circuit security, Security Detector SDK | https://openzeppelin.com |
| Nethermind | ZK verifier formal verification, privacy pool audits | https://nethermind.io |
