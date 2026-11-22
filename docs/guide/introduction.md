# Sigillr

<div class="brand-lockup">
  <img class="on-light" src="images/brand/sigillr-lockup-light.svg" alt="Sigillr, a seal you can check">
  <img class="on-dark" src="images/brand/sigillr-lockup-dark.svg" alt="Sigillr, a seal you can check">
</div>

**A seal you can check, with nothing personal behind it.** Sigillr (Latin *sigillum*: seal, signet)
is a public identity registry. People and organisations register the public keys of their
certificates themselves, under a pseudonymous `user_id`. The site logs them in with that same
certificate (mutual TLS). Anyone can then look up a key or certificate and see which identity it is
bound to, whether it is still valid, and its whole history.

## What it does

- **Directory:** pseudonymous identities, people and organisations, with the tags they choose.
- **Key registry:** which keys belong to which `user_id`, and whether each is active, expired or revoked.
- **Identity provider:** an OpenID Connect provider whose ID tokens carry only the issuer and the `user_id`.

The source of truth is a repository of YAML records and public keys. Only the service writes to
it and anyone can read it. The relational index is disposable and rebuilt from that repository.

## No personal data

Sigillr holds no personally identifiable data: no email address, no profile photo, no legal name. A
`user_id` or pseudonym that looks like an email address, including obfuscated forms, is refused.
Values you publish stay in the public history permanently, and you are told so, and
acknowledge it, before anything is published.

## What it is not

- It is not a certificate authority: it issues no certificates.
- It never stores private keys.
- It is not a credential platform or a wallet.

Sigillr is open-source software (MIT OR Apache-2.0) that anyone can run. The public instance is run
by provr at `sigillr.provr.dev`.

> Status: the project is at its start. This guide describes what Sigillr is and will do;
> [Running an instance](running.md) says what the current build does.
