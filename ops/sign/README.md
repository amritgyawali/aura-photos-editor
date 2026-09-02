# Code signing

Two platforms, two mechanisms, one rule: **the private key never touches a developer's machine and
never appears in this repository.**

## Windows

Authenticode over the installer and the executable, with an EV certificate held in a hardware token
or a cloud HSM. `signtool` with `/fd sha256 /tr <timestamp> /td sha256`.

The timestamp is not optional. Without it, every copy of the application stops verifying the day the
certificate expires — which for a photographer who installed it two years ago is an operating system
refusing to launch software that has not changed.

## macOS

`codesign --options runtime --timestamp` with a Developer ID Application certificate, hardened
runtime on, then notarisation (`ops/notarise/`), then `stapler`.

The hardened runtime is what makes the notarisation possible and it is also what breaks a plugin
host that loads unsigned code. AURA loads no third-party code at runtime, which is why it can take
the strict option.

## What is signed

| Artefact | Windows | macOS |
|---|---|---|
| The application | Authenticode | codesign + notarise + staple |
| The installer | Authenticode | codesign + notarise + staple |
| Model packs | ed25519 over `models.lock`, platform-independent | same |
| The Photoshop plugin | Authenticode over the `.ccx` | codesign over the `.ccx` |
| The Lightroom plugin | **Unsigned** | **Unsigned** |

The Lightroom row is not an oversight. Lightroom loads a `.lrdevplugin` as source and has no
signing mechanism for one; signing it would be signing a folder nothing checks the signature of.

## Model packs are signed separately, and that is the point

A model pack is verified by `aura-models` at install time — ed25519 over the manifest, then sha256
per file, then the model card, in that order. It is deliberately **not** part of the application
signature.

Section 6.4: "a model rollback must be possible without downgrading the app". If a model pack were
inside the application bundle, rolling a bad model back would mean rolling the application back —
which would take a photographer's bug fixes with it.

## The key

In a hardware token (Windows) or a cloud HSM (macOS). CI signs by calling a signing service; no CI
runner holds a key. A key that lives on a runner is a key that lives in every snapshot of that
runner.

`ops/sign/sign.sh` is the entry point and takes the artefact path plus a platform. It refuses to run
without the environment the signing service needs, rather than producing an unsigned artefact that
looks signed because the script exited 0.
