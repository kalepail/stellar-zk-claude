# ZK Docs Guide

Stellar + ZK protocol, tooling, ecosystem, onboarding, and security references.

Does not own game-specific docs or proving-system analysis — those live in `docs/games/`.

## Canonical Sequence (12 files)
| # | File | Topic |
|---|------|-------|
| 00 | `00-OVERVIEW.md` | System map and orientation |
| 01 | `01-PROTOCOL-FOUNDATIONS.md` | Core protocol concepts and terminology |
| 02 | `02-GITHUB-REPOSITORIES.md` | Repository landscape and project links |
| 03 | `03-PROVING-SYSTEMS.md` | Proving-system choices and tradeoffs |
| 04 | `04-PRIVACY-POOLS.md` | Privacy pool concepts and implementation |
| 05 | `05-DEVELOPER-TOOLS.md` | Developer tooling and workflow references |
| 06 | `06-PARTNERSHIPS.md` | Ecosystem and partner landscape |
| 07 | `07-USE-CASES.md` | Practical usage patterns |
| 08 | `08-ARTICLES-AND-MEDIA.md` | External reading and media |
| 09 | `09-GETTING-STARTED.md` | Onboarding path for builders |
| 10 | `10-SECURITY-BEST-PRACTICES.md` | Security model and guidance |

## Cross-References
- Integer math for ZK game logic: `docs/games/asteroids/integer-math-reference.md`
- Noir vs RISC Zero analysis: `docs/games/asteroids/noir-vs-risczero-analysis.md`

## Edit Routing
| Change | Edit |
|--------|------|
| Architecture | `01-PROTOCOL-FOUNDATIONS.md` + update `00-OVERVIEW.md` |
| Proving stack | `03-PROVING-SYSTEMS.md` |
| New tool/SDK | `05-DEVELOPER-TOOLS.md` |
| Onboarding | `09-GETTING-STARTED.md` |
| Security | `10-SECURITY-BEST-PRACTICES.md` |

## Maintenance
- Keep section numbering (`00`–`10`) stable unless doing an explicit restructure.
- Add new files to this README immediately.
