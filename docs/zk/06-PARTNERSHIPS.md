# Partnerships & Integrations

## Nethermind

**Website**: https://nethermind.io
**Role**: Blockchain engineering firm with deep ZK and Ethereum expertise

### Key People
- **Matteo Lisotto (Oghma)** — Primary engineer on stellar-risc0-verifier. Built the entire verifier stack.
- **Nick Dimitriou (NiDimi)** — Applied cryptography researcher (UCL MSc). Meridian 2025 speaker.
- **Alex Abdugafarov (frozenspider)** — Internal code reviewer.
- **Lucian Stroie** — Meridian 2025 co-presenter.

### Contributions to Stellar ZK

1. **RISC Zero verifier deployment** on Soroban (September 2025) — full architecture: Router + Groth16 Verifier + Mock Verifier + TimeLock Controller
2. **Private payment solutions** development with SDF
3. **Anti-abuse mechanisms** based on privacy pools proposal
4. **Cross-chain bridge** infrastructure with Boundless and Wormhole
5. **Formal verification** expertise (proved honesty of ZKsync's verifier using EasyCrypt)
6. **OpenZeppelin integration** — uses `stellar-contracts` v0.6.0 for access control in the verifier system

### Key Presentations
- **Meridian 2025**: "Eliminating Transaction Limits with ZK"
  - Speakers: Lucian Stroie, Matteo Lisotto (Nethermind)
  - Video: https://www.youtube.com/watch?v=6GQjFg7XJ8U
- **Meridian 2025**: "Private Transactions for Stellar"
  - Speaker: Nick Dimitriou
  - Video: https://www.youtube.com/watch?v=JFgDnMBTwAE

---

## Boundless (by RISC Zero)

**Website**: https://boundless.xyz
**Role**: Decentralized ZK proof marketplace

### Integration Details

- **Launched**: Incentivized testnet July 2025, Mainnet September 2025
- **On Stellar**: RISC Zero verifiers deployed September 18, 2025
- **How it works**:
  1. Requestors submit proof requests (from any chain)
  2. Provers compete to generate proofs using GPUs
  3. Settlement happens on destination chain (including Stellar)

### "The Signal" Initiative
An industry partnership to ZK-prove every blockchain:
- Partners: Wormhole, Base, Linea, Optimism, BOB, Unichain, EigenLayer, Stellar
- Goal: Condense each network's finality into one proof
- Supported by $200K+ in ETH commitments

### Key Resources
- Binance article: https://www.binance.com/en/square/post/31145887400161
- Lumexo coverage: https://blog.lumexo.io/boundless-stellar-nethermind-risc-zero-integration/
- BlockEden analysis: https://blockeden.xyz/blog/2026/01/14/boundless-risc-zero-decentralized-proof-market-zk/

---

## Wormhole

**Role**: Cross-chain messaging protocol with ZK verification

### Integration with Stellar

- **Boundless NTT Verifier**: Allows ZK proofs to accompany Wormhole Guardian signatures
- **How it works**:
  1. Token flow originates on Ethereum
  2. Boundless generates ZK proof of Ethereum consensus (TSEth)
  3. Destination chain verifies both ZK proof AND Wormhole signatures
  4. Two-of-two security policy for maximum safety
- **Blog**: https://wormhole.com/blog/boundless-partners-with-wormhole-to-launch-zk-network-powered-by-risc-zero

---

## OpenZeppelin

**Role**: Security tooling and smart contract standards

### Contributions

1. **Confidential Token Association**: Co-founding member with SDF, Zama, and Inco
   - Developing framework for adding confidentiality to token standards
   - Prototyping first Stellar-native confidential tokens
   - **Note**: `openzeppelin-confidential-contracts` on GitHub (143 stars) targets Ethereum FHE only (Zama FHEVM), not Stellar
2. **Stellar Contracts Library**: `OpenZeppelin/stellar-contracts` (75 stars, actively maintained)
3. **Soroban Security Detectors SDK**: `OpenZeppelin/soroban-security-detectors-sdk` (6 stars) — automated vulnerability detection
4. **Monitor**: `OpenZeppelin/openzeppelin-monitor` (119 stars) — on-chain monitoring with Stellar support
5. **UI Builder**: `OpenZeppelin/ui-builder` (31 stars) — frontend components with Stellar support
6. **Relayer Service**: Transaction submission with parallel processing
7. **Noir Circuit Security Guide**: Developer guide for building safe Noir circuits
   - URL: https://www.openzeppelin.com/news/developer-guide-to-building-safe-noir-circuits

---

## Zama

**Role**: Homomorphic encryption technology

### Contributions
- **Confidential Token Association** member
- Working on confidential tokens that hide balances and transfer amounts
- FHE (Fully Homomorphic Encryption) integration possibilities

---

## Reclaim Protocol

**Website**: https://reclaimprotocol.org
**Repository**: https://github.com/reclaimprotocol/zkfetch-stellar-example
**Docs**: https://docs.reclaimprotocol.org/zkfetch/stellar

### Integration
- ZK proof generation for external data verification on Stellar
- Fetches data from APIs (CoinGecko, Trading Economics, etc.) with cryptographic proofs
- Verifies proofs on-chain via Soroban contracts
- Enables trustless oracle-like functionality

---

## Rather Labs - "Interstellar"

**Twitter**: https://x.com/rather_labs/status/1932081682439397868
**Description**: Proposal to bridge Noir ZK language with Stellar

### What It Does
- Write circuits in Noir
- Prove with BLS12-381
- Verify on Soroban
- Announced June 2025

---

## Confidential Token Association

**Members**: SDF, OpenZeppelin, Zama, Inco
**Goal**: Develop framework for adding confidentiality to widely adopted token standards
**Impact**: First Stellar-native confidential tokens, cross-platform privacy interoperability

---

## Human Network (formerly Mishti Network)

**Website**: https://human.tech
**Role**: Decentralized identity infrastructure with ZK nullifiers

### Capabilities
- Human Keys: Private keys derived from user identities
- ZK Nullifiers: Privacy-preserving Sybil resistance
- Secured by $3B+ in restaking (EigenLayer, Symbiotic)
- 27 independent nodes, 1.2M+ Human Keys issued

---

## Veridise

**Website**: https://veridise.com
**Role**: Security audit firm for Soroban contracts

### Contributions
- Multiple Soroban security audits
- Published comprehensive security guidance for ZK-enabled applications
- Blog: https://veridise.com/blog/audit-insights/building-on-stellar-soroban-grab-this-security-checklist-to-avoid-vulnerabilities/

---

## Certora

**Role**: Formal verification for Soroban smart contracts
- First WASM-powered platform to support Certora verification
- Has secured ~$25B in Ethereum protocols
- **Continuous integration service**: Not just one-time audits — provides ongoing formal verification as contracts evolve
- Supports automated property checking for ZK verifier contracts

---

## Space and Time

**Role**: Data indexing with ZK data verification

### Integration with Stellar
- Provides ZK-verified data indexing for Stellar blockchain data
- Enables trustless querying of on-chain state with cryptographic proofs
- Relevant for ZK applications that need verified historical data

---

## Inco

**Role**: Confidential computing infrastructure

### Contributions
- **Confidential Token Association** member (alongside SDF, OpenZeppelin, Zama)
- Working on confidential computing primitives
- FHE (Fully Homomorphic Encryption) integration for encrypted on-chain computation
- Contributing to the ERC-7984 Confidential Token Standard

---

## Interoperability Standards Organization for Digital Assets

**Partners**: Stellar, MIT, Chainlink, Wormhole, Canton
**Description**: Cross-industry partnership for interoperability standards
- Developing standards for cross-chain ZK proof verification
- MIT providing academic research backing
- Chainlink and Wormhole contributing cross-chain messaging expertise
- Canton bringing enterprise blockchain perspective

---

## Ecosystem Privacy Projects

### Moonlight
- Privacy-focused project building on Stellar
- Exploring confidential transactions and private payment channels

### Amon Privacy
- Community-driven privacy initiative on Stellar
- Working on privacy-preserving payment workflows

### Human Network (formerly Mishti Network)
**Website**: https://human.tech
- Decentralized identity infrastructure with ZK nullifiers
- Human Keys: Private keys derived from user identities
- ZK Nullifiers: Privacy-preserving Sybil resistance
- Secured by $3B+ in restaking (EigenLayer, Symbiotic)
- 27 independent nodes, 1.2M+ Human Keys issued

---

## Stellar Community Fund (SCF) v7.0

**URL**: https://communityfund.stellar.org
**Relevance**: Primary funding mechanism for ZK ecosystem projects

### Structure
- **4-tranche milestone-based funding**: Projects receive funds as they hit milestones
- **Instawards**: Up to $15K for smaller contributions and quick wins
- Multiple ZK-related projects have been funded through SCF
- Community voting determines project selection
