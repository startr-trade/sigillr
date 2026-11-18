<!-- kanbanr:begin — generated, edits here are overwritten -->

## This project is tracked with kanbanr

The board is the system of record for scope, reasoning, progress and documentation. It is a separate git repository beside this one. **Recover from it at the start of every session** and record work there as you go — this block is a pointer, not a copy.

The board is published at <https://github.com/startr-trade/sigillr-board>. Clone it beside this repository as `sigillr.kanbanr` (`git clone https://github.com/startr-trade/sigillr-board ../sigillr.kanbanr`) and kanbanr finds it.

**Why this project exists.** sigillr (Latin sigillum: seal, signet) is a public identity registry. People and organisations register the public keys of their certificates themselves under a pseudonymous user_id; the site logs them in with that very certificate (mTLS); anyone can verify which identity a key or certificate is bound to. It holds no personally identifiable data. It is also an OpenID Connect provider whose ID tokens carry only its issuer and the user_id. It is the global identity registry koshr's identity claim rests on. sigillr is open-source software anyone can run; the public instance is run by provr at sigillr.provr.dev.

**What it commits to** (work items link these by id):

- `G-0` The system stays operable and maintainable
- `G-1` Self-registration, where a person registers a key by logging in with the certificate it belongs to (mTLS)
- `G-2` Public verification, where anyone looks up a key or certificate and gets the bound identity and its status
- `G-3` Revocation and expiry are visible to verifiers at once, including revocation by a certificate's issuing CA (CRL and OCSP)
- `G-4` Organisations are first-class identities, with members
- `G-5` koshr verifies actor ids and signing keys against sigillr, with caching and a bounded lifetime
- `G-6` Each identity records its proofing level (self-asserted or verified)
- `G-7` sigillr is an OpenID Connect Core 1.0 provider whose identity claims are only iss (sigillr) and sub (the user_id), with login by the registered certificate
- `G-8` SSH and GPG signing keys can be bound, each identity has a did:web document, Sigstore signatures resolve to sigillr identities, and a user may use their user_id as a NIP-05 name

**Deliberately out of scope** — do not propose these as gaps:

- Issuing certificates (acting as a certificate authority)
- Storing private keys, ever
- Holding personally identifiable data (no email addresses, no profile photos, no legal names)

**Constraints:**

- MIT OR Apache-2.0 licence; written in Rust
- The brand follows the startr.trade family guide
- Hosted only by provr (the public instance at sigillr.provr.dev)
- The source of truth is a git repository of YAML records and public keys; only the service writes to it, and anyone can read it
- The relational index is disposable, rebuilt from the repository, and used only for faster responses
- A user_id or pseudonym that looks like an email address (including obfuscated forms) is refused

**How work moves here** — each stage asks only for what it needs; `kanbanr check <CODE>` names what the next one still lacks:

- **Plan** — Say what it is and why it matters.
- **Design** — What it must do, agreed before it is built.
- **Develop** — Build it — how and where settled, a test named for every requirement.
- **Test** — Test it against its requirements.
- **Review** — Review it with the evidence in hand.
- **Released** — Shipped in a release.

**Before starting an item:** `kanbanr lessons --for <CODE>` — what this project already learned. **Before calling one done:** `kanbanr check <CODE>`.

| Question | Command |
|---|---|
| What is planned, in progress and done? | `kanbanr board` |
| What can I pick up now? | `kanbanr ready` |
| Why does this item exist, and how is it verified? | `kanbanr feature show <CODE>` |
| Why is the architecture like this? | `kanbanr adr list` |
| Why does this line of code exist? | `kanbanr why <file>:<line>` |
| What did the last wave cost? | `kanbanr retro <MS-00x>` |

Regenerate this block with `kanbanr claude sync`.

<!-- kanbanr:end -->
