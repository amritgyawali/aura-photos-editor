# ADR-0063 - TLS, and the provider catalogue behind the first-run setup screen

- **Status:** accepted
- **Date:** 2026-09-04
- **Deciders:** CTO, SEC (Security & Privacy Engineer), PM
- **Phase:** post-30

## Context

Phase 04 shipped a governed cloud gateway with four providers, and both halves of that
sentence had a shape that made sense at the time and does not any more.

**Four providers, written out in four modules.** Anthropic, OpenAI, Google and one
"compatible server" text field, offered in a dropdown that had those four strings
compiled into it in three places: `ProviderKind`, `build_provider` in `aura-app`, and
`PROVIDER_CHOICES` in the settings panel. That is a defensible shape while "which model"
is an engineering decision. It stops being defensible the moment it is a photographer's
decision - somebody who already pays for Groq, or whose studio has a Mistral contract,
or who runs Ollama on the machine under the desk, should not be told the product supports
three vendors and a text box.

**No TLS.** ADR-0009 section 2 recorded the waiver: the hand-written HTTP/1.1 client in
`aura_cloud::http` reaches `http://` endpoints, which in practice means a local or
studio-network OpenAI-compatible server, and `Connector` is the seam a TLS stack lands in
"when the supply-chain review clears". Sixteen of the nineteen providers in the catalogue
below are HTTPS-only. **A setup screen that collects a key it cannot use is worse than no
setup screen**, so the two decisions are one decision.

## Decision

### 1. The provider list is data

`aura_cloud::catalog` holds one row per provider: the wire format, the endpoint, whether
the endpoint is the photographer's to change, whether a key is required, what the key
looks like, whether the models can see a photograph, and three models with three prices.
`catalog::build` is the only place in the product that turns a choice into something that
can speak to a vendor, and `aura-app` does no matching of its own. Adding the twentieth
provider is a row in that table.

Nineteen ship: Anthropic, OpenAI, Google, Azure OpenAI, OpenRouter, Groq, Mistral,
DeepSeek, xAI, Together, Fireworks, DeepInfra, Cerebras, Moonshot, NVIDIA NIM, Perplexity,
Ollama, LM Studio, and "my own server". Three wire formats: Anthropic's Messages API,
Google's `generateContent`, and OpenAI's Chat Completions - which everybody else copied,
and which `Dialect` now distinguishes four ways rather than two.

### 2. Prices are defaults, not measurements

The prices in that table are the vendors' own published list prices at the time it was
written. Nothing in this repository has ever billed a provider. Two things keep the claim
honest, and both were already true in phase 04: the price is only ever used to **refuse**
a call before it is made, and the audit row records the tokens the provider said it
actually billed - so the spend meter a photographer reads is never an estimate. A studio
whose contract differs edits the table, and `budget::PRICE_TABLE_VERSION` says which
table a stored row was priced under.

Model names are the same shape with a weaker claim. A vendor renames a model roughly
every quarter, and a name that has moved produces one clean 404 rather than a wrong
answer. Every one is overridable per tier from the setup screen.

### 3. The choice is persisted; the key still is not

`aura-app`'s `ai_settings` writes the provider, the endpoint and the three model names to
the `setting` table - which has been in migration 0001 since phase 01 and had never been
written to. No migration is needed and none was added.

The key is not in that row and cannot be. It stays in the operating system's credential
store, filed per provider, so a photographer can keep three of them and switch without
retyping anything. A catalog copied to a second machine therefore carries the *choice*
and not the *secret*, which is the correct split between the two.

Phase 04 kept the provider choice in memory only. The symptom was that a photographer
picked Google, pasted a key, closed the application, and reopened it pointed at Anthropic
with no key - which reads as the key having been lost.

### 4. TLS ships, and the crypto is pure Rust

`aura_cloud::tls::TlsConnector` implements the `Connector` port ADR-0009 said it would.
`HttpTransport` now holds one connector per scheme, so a photographer with a hosted key
and Ollama on the same machine does not have to restart to switch between them.

The crypto provider is `rustls-rustcrypto` rather than `ring` or `aws-lc-rs`, and this is
a constraint rather than a preference: both of the providers rustls ships with compile C
and assembly, and the reference build machine for this product **has no C toolchain at
all**. `cargo test --workspace` passes there only because every other dependency is pure
Rust. Linking either would make the cloud half of the product unbuildable on the machine
it is developed on.

**What that costs, said plainly.** `rustls-rustcrypto` is version `0.0.2-alpha` and its
own authors describe it as not yet production-grade. It has not had the audit history
`ring` has. This is the weakest dependency in the product and it sits in the most
security-sensitive path in the product. It is accepted because the alternative on offer
was not "`ring` instead" - it was "no TLS at all, and a setup screen that collects keys
for sixteen vendors it cannot reach". The mitigations are that the `Connector` port makes
the swap a one-file change the day a C toolchain is available on the build machines, and
that the feature can be turned off (`--no-default-features`) to get byte for byte the
transport phase 04 shipped.

**What does not change.** Certificate verification is on and there is no switch that
turns it off: no `dangerous_configuration` feature, no "accept invalid certificates"
setting, no environment variable. The roots are the Mozilla set from `webpki-roots`
rather than the operating system's store, so a studio behind a TLS-inspecting proxy sees
a certificate error rather than a silent interception - the right way round for a product
that sends photographs of other people's weddings.

### 5. The transport says what it can reach, and the screen repeats it

`Transport::schemes` is on the wire in `AiSetupStatusDto`. A build compiled without the
`tls` feature reports `["http"]`, and the setup screen puts a warning on every HTTPS
provider saying a key saved there would not be used. Phase 24's rule - an absent input is
ignorance, not permission - applied to a capability rather than to a mask.

### 6. The first-run screen may always be declined

`skip_ai_setup` is a separate command from `save_ai_setup` and records no provider.
Declining answers the question, so nobody is asked twice, and a photographer who pressed
"not now" is never shown a configured-looking Anthropic with no key behind it. Invariant
6 is unchanged: the product does a complete wedding with none of this.

## Consequences

- `crates/aura-app/src/contract/ipc.rs` gains four types (`AiModelDto`, `AiProviderDto`,
  `AiSetupStatusDto`, `SaveAiSetupInput`). That is a frozen contract; `contracts.lock` is
  re-locked in the same change, and this ADR is the record the rule asks for. It is the
  sixth amendment to a frozen contract in the product's history, after phase 09's
  `FaceRef`, phase 16's re-lock, phase 23's `Lens::coefficients`, phase 24's
  `Recipe::cleanup` and phase 27's `TicketStatus::Dismissed`.
- Four IPC commands are added: `list_ai_providers`, `ai_setup_status`, `save_ai_setup`,
  `skip_ai_setup`. `scripts/check-ipc-surface.sh` reports 263 = 263 = 263.
- The dependency tree gains rustls, `webpki-roots` and the RustCrypto primitives. None needs a
  C compiler. All but one are MIT, Apache-2.0, ISC or BSD-3-Clause and already inside the
  `deny.toml` allow list. The exception is `webpki-roots`, which is the Mozilla trusted-root
  certificate set and is therefore **data rather than code** - it carries CDLA-Permissive-2.0,
  which imposes no conditions on use or redistribution and no attribution requirement. It is a
  scoped `deny.toml` exception in the shape `jpeg-encoder`'s IJG exception already established,
  so a second CDLA dependency still fails the gate.
- `ProviderKind` grows from four variants to nineteen. The identifiers are also the
  credential-store account names, so the four that existed keep their spelling and no
  stored key is orphaned.
- **Nothing here is evidence that a provider works.** No call in this repository has ever
  reached a public vendor, this machine cannot compile the desktop shell, and the
  cassette transport is what every test uses. What is proved is the catalogue, the
  persistence, the refusals and the screen; what is not proved is a single successful
  round trip to any of the nineteen. The first real one reopens this ADR's criteria the
  way the first real camera file reopens phase 02's.
