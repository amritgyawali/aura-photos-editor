# AURA shop and licence server

The website customers buy AURA on, and the small server that hands them their licence key.
Deployed to Vercel; payments through Paddle (merchant of record, so Paddle charges and pays the
sales tax and VAT everywhere). ADR-0107.

```
public/       the website: product, pricing + checkout, key page, terms, privacy, refunds
api/licence   GET ?subscription=sub_…&email=…  or  ?transaction=txn_…&email=…  -> { key, … }
api/config    the public checkout settings
lib/          key signing (byte-for-byte what crates/aura-licence reads) and the Paddle reads
test/         `npm test` - includes a key the Rust crate asserts too
dev.mjs       `node dev.mjs` - local preview on http://localhost:3000
```

The server stores nothing. A key is derived from the Paddle subscription each time it is asked
for (ed25519 is deterministic), it runs to the end of the paid period plus `LICENCE_GRACE_DAYS`,
and the AURA app asks for the next one by itself a few days before that.

## Going live - in this order

1. **Paddle sandbox.** Sign up at <https://sandbox-vendors.paddle.com>. Create a product "AURA Photo
   Studio" with two prices: US$15 monthly and US$129 yearly. Note both price ids (`pri_…`).
   *Developer tools -> Authentication*: create a client-side token and an API key with
   read access to transactions, subscriptions and customers.
2. **Deploy.** Import this `shop/` folder as a Vercel project (framework: Other). Set the
   environment variables below, deploy, open the site, and buy with a Paddle test card
   (`4242 4242 4242 4242`, any future date, CVC `100`). You should land on `/key` with a key that
   AURA accepts (Advanced -> Licence).
3. **Paddle live.** Sign up at <https://vendors.paddle.com> as a sole proprietor with your legal
   name and Nepali ID, add your payout method (bank wire to a Nepali account, or Payoneer), and
   submit the website for domain approval - it checks for the pricing, terms, privacy and refund
   pages, which are all here. A custom domain (for example `aura-studio.com`) is approved more
   readily than `*.vercel.app`.
4. **Switch.** Repeat step 1's product, prices, token and API key in the live account, set
   `PADDLE_ENV=live` and the live values on Vercel, redeploy.
5. **Build AURA against it.** The desktop app learns where to renew from at build time:
   `AURA_LICENCE_SERVER=https://<your-domain>` when building the shell (see
   `docs/release-process.md`).

## Environment variables (Vercel -> Settings -> Environment Variables)

| Name | Value |
|---|---|
| `AURA_VENDOR_KEY` | The vendor seed, base64 - the contents of `vendor-licence.key.base64.txt` beside your vendor key. **Secret.** |
| `PADDLE_ENV` | `sandbox` or `live` |
| `PADDLE_API_KEY` | Paddle API key (server side). **Secret.** |
| `PADDLE_CLIENT_TOKEN` | Paddle client-side token (public) |
| `PADDLE_PRICE_MONTHLY` | `pri_…` of the US$15 monthly price |
| `PADDLE_PRICE_YEARLY` | `pri_…` of the US$129 yearly price |
| `DOWNLOAD_URL` | Where the installer is, e.g. a GitHub release asset (Vercel cannot host a 467 MB file) |
| `LICENCE_GRACE_DAYS` | Optional, default 7: days a key outlives its paid period |

Putting the vendor key on Vercel is the trade this design makes for automatic delivery: anybody who
obtains it can make keys. Keep it only there and in your offline backup, and never in this
repository. If it ever leaks, generate a new key pair, ship an AURA update with the new public key,
and re-issue.
