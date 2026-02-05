# ZK Use Cases on Stellar

Reference: https://stellar.org/blog/developers/5-real-world-zero-knowledge-use-cases

---

## 1. zkTokens / Confidential Tokens

### What
Tokens protected by ZK proofs that enable private transactions and balances while maintaining public ledger transparency that total inputs equal total outputs.

### Approaches
1. **Privacy Pool Based**: Deposit into shared pool, withdraw privately with ZK proof
2. **Encrypted Balances**: Encrypt amounts and balances, use homomorphic properties + ZK proofs for integrity
3. **Confidential Tokens (NEW)**: Full confidential token standard with encrypted balances, ZK range proofs, and selective disclosure

### Status on Stellar
- Privacy Pools prototype: **implemented** (see privacy-pools example)
- Confidential Token Association (SDF + OpenZeppelin + Zama + Inco): **actively developing**
- **Confidential Tokens demo**: Jay Geng presented at Meridian 2025 (https://www.youtube.com/watch?v=6NnDqVQYOHM)
- First wave: fungible tokens (stablecoins, governance tokens, SEP-41 tokens)

### Confidential Tokens (Meridian 2025 Demo)

Jay Geng's Meridian 2025 presentation demonstrated a working Confidential Token implementation with:

- **Encrypted Balances**: Token balances are encrypted on-chain, invisible to public observers
- **Confidential Transfers**: Transfer amounts hidden using encryption
- **ZK Range Proofs**: Prove that transferred amounts are valid (non-negative, within balance) without revealing the actual values
- **Sigma Proofs**: Cryptographic proofs for balance correctness
- **View Keys**: Allow selective disclosure of transaction details to specific parties
- **Auditor Keys**: Enable compliance officers to inspect transactions without breaking privacy for other users
- **Official prototype**: `stellar/stellar-confidential-token` (3 stars) and `jayz22/stellar-confidential-transfer` (2 stars) — both by Jay Geng (SDF)

### Confidential Token Standard (In Development)

Being developed by the Confidential Token Association (SDF + OpenZeppelin + Zama + Inco):

- **Encrypted balances**: Using FHE or ZK range proofs for on-chain confidential state
- **Operator patterns**: Delegated operations on confidential tokens
- **Freezable extensions**: Compliance-driven asset freezing capability
- **RWA extensions**: Real World Asset support with identity-linked compliance
- **Cross-platform**: Being designed for both Stellar and Ethereum ecosystems
- **Official prototype**: `stellar/stellar-confidential-token` by jayz22 (SDF)
- **Note**: The formal ERC standard number has not been verified. `OpenZeppelin/openzeppelin-confidential-contracts` (143 stars) targets Ethereum FHE (Zama FHEVM), not Stellar.

### Implementation Path
```
Option A: Privacy Pools (available now)
  - Use soroban-examples/privacy-pools as template
  - Groth16 + BLS12-381 + Circom

Option B: Confidential Tokens (in development)
  - ERC-7984 standard implementation
  - FHE + ZK range proofs
  - View keys for selective disclosure
  - Confidential Token Association specifications

Option C: Encrypted Balances (future)
  - Full homomorphic encryption primitives
  - Zama FHE integration
```

---

## 2. zkLogin - Zero-Knowledge Authentication

### What
Users prove ownership of credentials without transmitting secrets. Authenticate using Web2 identities (Gmail, OAuth) while remaining anonymous on-chain. Also includes **ring signature authentication** where users prove membership in a group without revealing which member they are.

### How It Works
1. User authenticates with familiar identity provider (Google, etc.)
2. Generate ZK proof of credential ownership
3. Blockchain verifies proof without learning email/identity
4. User gets on-chain access without managing private keys

### Ring Signature Approach (kLogin)
Ring signatures let someone prove membership in a group without revealing which member they are, enabling:
- Anonymous authentication
- Private transactions
- Secret ballots on a public blockchain

### Benefits
- Reduces phishing and data breaches
- No complex private key management for end users
- Bridges Web2 authentication with Web3 ownership
- Privacy-preserving onboarding

### Implementation Resources
- **kLogin Tutorial**: https://jamesbachini.com/privacy-on-stellar/
- **Video**: https://youtu.be/32xCKGrf3MI
- **Code**: https://github.com/jamesbachini/Stellar-BLS

### Implementation Path
```
1. Integrate with identity provider (OAuth/OIDC)
2. Build ZK circuit that proves credential possession
3. Deploy verifier contract on Soroban
4. Frontend generates proofs client-side
5. Verify on-chain, grant access
```

---

## 3. zkKYC - Selective Disclosure Compliance

### What
Users prove compliance attributes (accredited investor, US resident, sanctions-clear) without revealing underlying identity documents.

### How It Works
1. User completes KYC with a trusted third-party provider
2. Provider issues cryptographic credentials with proven attributes
3. User generates ZK proofs: "I am accredited" or "I am not sanctioned"
4. DeFi protocol verifies proof, approves access
5. No identity documents ever touch the blockchain

### Example Flow
```
User -> KYC Provider: Complete verification
KYC Provider -> User: Issue credential (accredited=true, country=US)
User -> DeFi Protocol: ZK proof (I'm accredited AND US-based)
DeFi Protocol: Verify proof, grant access
```

### Benefits
- Users manage their own KYC credentials
- Protocols get compliance without handling PII
- Regulators can verify compliance without seeing raw data
- Works across multiple protocols with one verification

### Implementation Resources
- **Selective Disclosure KYC Tutorial**: https://jamesbachini.com/selective-disclosure/
- **Video**: https://youtu.be/ajy3G_Y4l1w
- **Code**: https://github.com/jamesbachini/Selective-Disclosure-KYC

Uses BLS ring signatures to let users prove attributes privately without revealing full identity documents.

---

## 4. zkVoting - Zero-Knowledge Governance

### What
Private on-chain votes that are cryptographically verifiable while preserving individual vote privacy.

### How It Works
1. Voter proves right to vote via ZK proof (token holder, DAO member)
2. Vote is cast privately - no one can link vote to identity
3. Results are publicly verifiable
4. Individual votes remain secret

### Applications
- DAO governance
- Token holder voting
- Community proposals
- Potentially: national elections

### Implementation Path
```
1. Define eligibility criteria (token balance, membership)
2. Build ZK circuit for eligibility proof
3. Implement encrypted vote submission
4. Tally mechanism that preserves privacy
5. Public verification of results
```

---

## 5. zkCompute / zkVM - Verifiable Off-Chain Computation

### What
Move expensive computation off-chain, generate ZK proofs of correct execution, verify proofs on-chain efficiently.

### How It Works (RISC Zero on Stellar)
1. Write Rust program with business logic
2. Execute off-chain in RISC Zero zkVM
3. Generate STARK proof ("receipt") of correct execution
4. Submit proof to Soroban
5. On-chain verifier confirms computation was correct

### Use Cases
- Complex financial calculations
- Machine learning inference results
- Supply chain verifications
- Data aggregation and analytics
- Any computation too expensive for on-chain execution

### Benefits
- Removes transaction limits on computation
- Each node verifies one proof instead of re-executing everything
- Enables complex applications on a lightweight settlement layer
- Cross-chain data verification

### Implementation Path
```
1. Write Rust program for your computation
2. Compile for RISC-V target
3. Run in RISC Zero zkVM
4. Submit receipt to Soroban's RISC Zero verifier
5. Contract validates the proof and acts on results
```

---

## Additional Use Cases

### zkOracles (Reclaim Protocol)
- Fetch external API data with ZK proofs
- Verify data authenticity on-chain without trusted oracles
- Repository: https://github.com/reclaimprotocol/zkfetch-stellar-example

### Privacy-Preserving Payments
- Cross-border payments with hidden amounts
- Payroll without exposing salaries
- B2B transfers without revealing volumes
- Reference: https://stellar.org/blog/developers/financial-privacy

### Cross-Chain State Verification
- Prove Ethereum state on Stellar (and vice versa)
- Compress full chain history into a single proof
- Enable trustless bridges via Boundless + Wormhole
- **Steel (RISC Zero)**: ZK coprocessor for reading blockchain state inside zkVM proofs
- **"The Signal" Initiative**: Partnership to ZK-prove every blockchain's finality (Wormhole, Base, Linea, Optimism, BOB, Unichain, EigenLayer, Stellar)

### Financial Inclusion
- Microfinance: prove creditworthiness without exposing financial history
- Aid distribution: verify eligibility without revealing personal data
- Reference: Stellar ZK Learn Page (https://stellar.org/learn/zero-knowledge-proof)

### ZK-Verified Data (Space and Time / Reclaim)
- **Space and Time**: Data indexing with ZK verification for Stellar blockchain data
- **Reclaim Protocol**: Fetch external API data with ZK proofs of authenticity
- Enables trustless oracle functionality without centralized trust assumptions
- Use cases: price feeds, identity verification, off-chain data attestation

### Compliance-Preserving Privacy (Nethermind Roadmap)
Building on the 3-phase Nethermind roadmap:
- **Phase 1 (now)**: Privacy pools with association sets for basic compliance
- **Phase 2 (next)**: View keys allowing auditors to inspect specific transactions
- **Phase 3 (future)**: In-pool transfers maintaining compliance while preserving privacy
- Reference: Nick Dimitriou, Meridian 2025 (https://www.youtube.com/watch?v=JFgDnMBTwAE)
