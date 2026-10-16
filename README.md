# knot-sync

Keeps Knot zones in step with zone files from Git and with the mail addresses in a 389ds directory.

Every zone with a file is knot-sync's: the file is what the zone serves. On top of it, an address like `shop@a1b2.shtr.eu` only receives mail when `a1b2.shtr.eu` has an MX record, so every subdomain an address in the directory uses gets two records:

```text
a1b2.shtr.eu.  MX   10 mail.shotor.com.
a1b2.shtr.eu.  TXT  "v=spf1 mx -all"
```

A subdomain that no address uses any more loses them again.

## How it works

The wanted state of a zone is its file plus the contact records. A contact subdomain is a single label of letters, digits and hyphens below any zone knot-sync has a file for, taken from every value of the address attributes, `mailAliases` and `mailMaskedAddresses` by default. A name the file gives records of its own keeps the file's records alone.

The present state is the zone as Knot serves it, read by zone transfer. knot-sync sends the difference as one RFC 2136 update per zone, signed with its TSIG key, which Knot applies all or nothing: no record is missing while an RRset changes. A record changed by hand is put back on the next pass.

Some records are never knot-sync's, whatever the file says:

- the SOA and the DNSSEC records, which Knot keeps itself;
- `_acme-challenge` TXT records, which cert-manager writes.

It reconciles every zone:

- when it connects to the directory;
- when the directory reports a change, through a persistent search, a couple of seconds after the last change of a burst;
- when a zone file changes, checked every `KNOT_SYNC_ZONE_POLL_SECS`;
- every `KNOT_SYNC_INTERVAL_SECS`.

Without the directory nothing is reconciled, since the contact records would look unwanted. A persistent search cannot resume, so a lost connection is retried after 30 seconds and starts with a full reconcile. A pass that would delete more than `KNOT_SYNC_MAX_DELETES` records over all zones is refused and logged before any zone changes, and is tried again until the files or the limit change.

The addresses come from email-manager's virtual aliases and masked email, which Keycloak's LDAP federation writes to the directory. In both its list and JSON modes the directory holds one plain address per value in these attributes, which is all knot-sync reads.

## Configuration

Everything comes from the environment, or `.env` in development. Secrets are read from files.

| Variable                       | Default                           | Description                                          |
| ------------------------------ | --------------------------------- | ---------------------------------------------------- |
| `KNOT_SYNC_LDAP_URL`           |                                   | `ldap://` or `ldaps://` URL of the directory         |
| `KNOT_SYNC_LDAP_BIND_DN`       |                                   | the DN knot-sync binds as                            |
| `KNOT_SYNC_LDAP_PASSWORD_FILE` |                                   | a file holding the bind password                     |
| `KNOT_SYNC_LDAP_BASE_DN`       |                                   | where the users are searched                         |
| `KNOT_SYNC_LDAP_ATTRIBUTES`    | `mailAliases,mailMaskedAddresses` | the address attributes, comma separated              |
| `KNOT_SYNC_DNS_SERVER`         |                                   | `host:port` of the Knot primary                      |
| `KNOT_SYNC_ZONE_DIR`           |                                   | the directory of zone files, `<zone>.zone` each      |
| `KNOT_SYNC_ZONE_POLL_SECS`     | `30`                              | seconds between checks of the zone files             |
| `KNOT_SYNC_TSIG_KEY_NAME`      | `knot-sync`                       | the TSIG key's name in Knot                          |
| `KNOT_SYNC_TSIG_KEY_FILE`      |                                   | a file holding the base64 HMAC-SHA256 secret         |
| `KNOT_SYNC_MX_HOST`            | `mail.shotor.com`                 | the MX target                                        |
| `KNOT_SYNC_INTERVAL_SECS`      | `900`                             | seconds between full reconciles                      |
| `KNOT_SYNC_MAX_DELETES`        | `5`                               | the most records one pass may delete                 |
| `KNOT_SYNC_LOG_FORMAT`         | `pretty`                          | `pretty` or `json`                                   |
| `RUST_LOG`                     | `info`                            | log filter                                           |
| `SSL_CERT_FILE`                |                                   | the CA bundle that signs the directory's certificate |

## Directory and Knot

The bind user needs read access to the address attributes under the base DN and nothing else. 389ds must allow it the persistent search, which it supports natively.

A zone file is an ordinary zone file without `$ORIGIN`: the origin is the file's name. It needs NS records at the apex; its SOA is ignored.

Knot loads each zone once from a seed file holding only the SOA and the apex NS, and must never load a changed one: a changed zone file makes Knot drop every record that came by update. So the seed never changes, and the journal keeps the zone:

```yaml
zone:
  - domain: shtr.eu
    file: shtr.eu.seed
    zonefile-sync: -1
    zonefile-load: difference
    journal-content: all
    acl: [knot-sync]
```

The TSIG key needs updates and zone transfers on every zone:

```yaml
acl:
  - id: knot-sync
    key: knot-sync
    action: [update, transfer]
```

## Development

```sh
cp .env.example .env

cargo dev
```

```sh
cargo lint-fmt && cargo lint-clippy

cargo test
```

## Releases

Conventional commits. A push to `main` with a releasable commit bumps the version with commitizen, tags it and publishes `ghcr.io/shotor/knot-sync:<version>` with a GitHub release.
