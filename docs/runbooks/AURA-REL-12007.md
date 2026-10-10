# AURA-REL-12007 - A subscription could not be renewed from the licence server

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA could not renew your subscription just now. Nothing has changed; it will try again, or paste
your current key from the shop.

## What actually happened

Within ten days of a subscription key's end, AURA asks the licence server
(`GET <server>/api/licence?subscription=<id>&email=<email>`) for the current key. The request
failed, the server refused (for example a cancelled subscription, or a payment still being
retried), or the answer was not a genuine, later key for the same subscription and person. The key
already on the machine is untouched.

## What to do

Check the internet connection and try **Renew now** in Settings -> Licence. If the subscription
is active, the photographer can also open the shop's "Get my key" page with their subscription id
and email and paste the key. A subscription the server reports as cancelled ends at the close of
its paid period; editing keeps working after that, exporting needs a renewed subscription.

## Where it comes from

ADR-0107, `docs/adr/ADR-0107-selling-aura-from-nepal.md`.
