# ADR-0107: Selling AURA from Nepal - Paddle, subscriptions that renew themselves, an individual signing certificate, version 1.0.0

Status: accepted.
Date: 2026-10-08

## Problem

ADR-0105 gave AURA an installer and offline licence keys, and left four things to the seller: a shop,
a code-signing certificate, the licence agreement's blanks, and a version number. The seller is an
individual based in Kathmandu, Nepal, and chose a subscription at US$15 a month or US$129 a year.
Where the seller is based decides most of the rest.

## Decision

### The shop: Paddle, not Lemon Squeezy

Lemon Squeezy was the first choice and cannot be used. Its rule is that a merchant must be able to
receive a bank payout in one of its listed countries or a PayPal payout, and "if you don't see your
country listed by PayPal or in the list below, ... you won't be able to use Lemon Squeezy" (Lemon
Squeezy, *Supported countries*). Nepal is not on the bank list, and PayPal accounts in Nepal are
send-only. Stripe is not available in Nepal either.

**Paddle** supports sellers in Nepal (Nepal is not on its list of unsupported supplier countries)
and pays out by bank wire or Payoneer. Like Lemon Squeezy it is the merchant of record: Paddle sells
to the customer, collects and pays sales tax and VAT everywhere, and handles refunds and chargebacks.
Its domain approval needs a live HTTPS site with the product, pricing, terms (with the seller's legal
name), privacy and refund policies, all of which `shop/public/` provides.

### Keys for a subscription, without storing anything

`shop/` is a Vercel project: the website plus `GET /api/licence`. Given a subscription id (`sub_...`)
or, straight after checkout, the order id (`txn_...`), and the email address paid with, it reads the
subscription from Paddle's API and returns an ordinary AURA key:

- `id` is the subscription id, `name` and `email` the customer's, `issued` the subscription start;
- `expires` is the end of the **current paid period plus 7 days' grace**, so a renewal payment that
  Paddle is still retrying does not stop anybody's export;
- refused for a cancelled or paused subscription, and for any mismatch, with one message that does
  not reveal whether the number or the email was wrong.

Ed25519 signatures are deterministic, so the same subscription and period always give the same key:
the server stores nothing, has no database, and can show a customer their key again any time. The
signing code in `shop/lib/licence.js` is held byte-for-byte to the Rust reader by a key both test
suites assert.

### Renewal inside the app

Within 10 days of a subscription key's end, AURA asks `<server>/api/licence?subscription=...&email=...`
for the current key (`licensing::refresh_licence`, through `aura-cloud`'s HTTP transport, so the
"no socket outside aura-cloud" rule holds). The answer is accepted only if it decodes against the
shipped public key, carries the same subscription id and email, and runs later than the key it
replaces; anything else - an outage, a refusal, a stale or forged answer - leaves the existing key in
place and reports `AURA-REL-12007`. The panel offers **Renew now**. The server address is compiled in
from `AURA_LICENCE_SERVER` and can be overridden by the same environment variable at run time. This
is the one network request AURA makes without a feature being switched on, and `docs/privacy.md` and
the licence agreement both say so: a subscription number and an email, nothing about photographs.

### The certificate

Microsoft Trusted Signing is not offered to individuals in Nepal. The choice is an individual
code-signing certificate in a certificate authority's cloud HSM - Certum Standard Code Signing in the
Cloud (individuals verify with an ID document and a utility bill), signing through SimplySign Desktop
and `signtool`. `ops/sign/README.md` has the exact command. Buying it, and the identity check, are
the seller's to do.

### The licence agreement and the website's legal pages

`legal/EULA.txt` names the seller (Amrit Gyawali, Kathmandu, Nepal), the support address, Nepali law
and Kathmandu courts (preserving a consumer's right to sue where they live), and now describes the
subscription: Paddle as reseller, automatic renewal, cancellation to the end of the paid period,
editing kept after it ends. The website shows the same text (`npm test` fails if the two drift), a
14-day refund on a first payment, and a privacy notice. **A Nepali lawyer should review both before
the first sale**; nothing here is legal advice.

### Version 1.0.0

The workspace, the shell, the window and the installer are 1.0.0: a product people pay for. Later
fixes are 1.0.x, new features 1.x.0.

## What is not done, and cannot be from here

Creating the Paddle accounts, passing their verification, buying the certificate, registering a
domain, deploying to Vercel with the vendor key as a secret, and the lawyer's review. `shop/README.md`
lists them in order.
