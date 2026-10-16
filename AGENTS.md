# Project instructions

## Product

**knot-sync** keeps Knot zones in step with zone files from Git and with the mail addresses in 389ds. Every zone with a file in `KNOT_SYNC_ZONE_DIR` is its own: the file is what the zone serves, plus an `MX 10 mail.shotor.com` and a `TXT "v=spf1 mx -all"` on every `<label>.<zone>` an address of the form `<x>@<label>.<zone>` uses, unless the file gives that name records of its own. The tool reads the present state by zone transfer from the Knot root and sends the difference as one RFC 2136 update per zone, signed with its TSIG key. The design is shotor/infra#143.

- Never its records: the SOA, the DNSSEC records (Knot's) and `_acme-challenge` TXT (cert-manager's).
- The addresses come from `KNOT_SYNC_LDAP_ATTRIBUTES`, `mailAliases` and `mailMaskedAddresses` by default: the plain attributes that email-manager's virtual aliases and masked email write one address per value. In JSON mode its `email-manager-ldap-mapper` still writes them beside the JSON (`mailIdentity`, `mailMaskedIdentity`), which the tool never reads.
- A persistent search on any of those attributes is a nudge only. The tool reconciles every zone after every connect, on every nudge, on a changed zone file and every `KNOT_SYNC_INTERVAL_SECS`, since a persistent search cannot resume. Without the directory it reconciles nothing.
- A pass that would delete more than `KNOT_SYNC_MAX_DELETES` records over all zones is refused before any zone changes.
- Knot loads only an unchanging seed per zone: a changed zone file makes it drop every record that came by update.
- Secrets reach the process as files (`KNOT_SYNC_LDAP_PASSWORD_FILE`, `KNOT_SYNC_TSIG_KEY_FILE`), never as environment values.
- LDAPS trusts the system roots through `rustls-native-certs`, which reads `SSL_CERT_FILE`: in the cluster it points at the CA that signs 389ds's certificate.

## Conventions live in the skills

- **`shotor-rust`**: formatting and whitespace, file layout, `__test.rs` siblings, naming, environment variables.

Never single-line `if`s: braces with the body on its own lines.

## Tooling

Everything is a Cargo alias in `.cargo/config.toml` (no Makefile). Lint is two commands because aliases can't chain: `cargo lint-fmt && cargo lint-clippy`. Configuration comes from the environment, `.env` in development (`.env.example` lists every variable).

## Releases

Conventional commits. A push to `main` with a releasable commit bumps the version with commitizen, tags it with the repository's deploy key (`RELEASE_DEPLOY_KEY`), since a tag pushed with `GITHUB_TOKEN` starts no workflow, and the tag publishes `ghcr.io/shotor/knot-sync:<version>` and a GitHub release.

## Avoid

- Do not add dependencies without asking first.
- Do not commit, push, or otherwise publish changes.
- Do not modify unrelated files.
- Do not create additional documentation unless requested.
