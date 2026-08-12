//! `model-sign` - the offline signing tool for `models.lock`.
//!
//! Two keys exist and they are not interchangeable.
//!
//! The **release key** is generated on an offline machine, never leaves it, and
//! never enters this repository or CI. `model-sign sign --key <file>` uses it.
//! The corresponding public key is compiled into shipping builds through the
//! `AURA_RELEASE_PUBLIC_KEY` environment variable at build time.
//!
//! The **development key** is derived deterministically from a public seed
//! phrase, printed below and in the source. It exists so that anybody can rebuild
//! and re-sign the placeholder models in this repository, and it is worthless for
//! anything else: everyone who can read the repository can compute it. A build
//! that trusts it is a development build by definition.
//!
//! ```text
//! model-sign keygen --dev              print the development key pair
//! model-sign keygen --out key.bin      generate a release key, offline
//! model-sign sign --dev <file>         sign with the development key
//! model-sign sign --key key.bin <file> sign with a release key
//! model-sign verify --pub <hex> <file> <signature-file>
//! ```
//!
//! Panics are permitted in this crate: it is an operator tool run by a human at a
//! terminal, not product code, and a wrong argument should stop it loudly.

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// The seed phrase for the development key. Public on purpose - see the module
/// documentation. Changing it invalidates every signature in the repository.
const DEV_SEED_PHRASE: &str = "AURA development signing key, not for release";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("keygen") => keygen(&args[1..]),
        Some("sign") => sign(&args[1..]),
        Some("verify") => verify(&args[1..]),
        _ => {
            eprintln!(
                "usage:\n  \
                 model-sign keygen [--dev | --out <key-file>]\n  \
                 model-sign sign [--dev | --key <key-file>] <file> [--out <signature-file>]\n  \
                 model-sign verify --pub <hex> <file> <signature-file>"
            );
            Err("no subcommand".to_string())
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("model-sign: {message}");
            ExitCode::FAILURE
        }
    }
}

fn dev_key() -> SigningKey {
    let mut hasher = Sha256::new();
    hasher.update(DEV_SEED_PHRASE.as_bytes());
    let seed: [u8; 32] = hasher.finalize().into();
    SigningKey::from_bytes(&seed)
}

fn keygen(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--dev") {
        let key = dev_key();
        println!("development key pair, derived from a public seed phrase");
        println!("private: {}", hex(&key.to_bytes()));
        println!("public:  {}", hex(key.verifying_key().as_bytes()));
        return Ok(());
    }

    let out = flag(args, "--out").ok_or("keygen needs --dev or --out <key-file>")?;
    // A release key must come from the operating system's randomness, on the
    // offline machine that will keep it. Deriving it from anything reproducible
    // would defeat the point of the whole chain.
    let mut seed = [0u8; 32];
    getrandom(&mut seed)?;
    let key = SigningKey::from_bytes(&seed);
    fs::write(&out, key.to_bytes()).map_err(|err| format!("could not write {out}: {err}"))?;
    println!("public: {}", hex(key.verifying_key().as_bytes()));
    println!("private key written to {out} - keep it offline, never commit it");
    Ok(())
}

fn sign(args: &[String]) -> Result<(), String> {
    let key = if args.iter().any(|a| a == "--dev") {
        dev_key()
    } else {
        let path = flag(args, "--key").ok_or("sign needs --dev or --key <key-file>")?;
        let bytes = fs::read(&path).map_err(|err| format!("could not read {path}: {err}"))?;
        let seed: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| format!("{path} is not a 32 byte ed25519 key"))?;
        SigningKey::from_bytes(&seed)
    };

    let target = positional(args).ok_or("sign needs a file to sign")?;
    let payload = fs::read(&target).map_err(|err| format!("could not read {target}: {err}"))?;
    let signature = key.sign(&payload);

    let out = flag(args, "--out").unwrap_or_else(|| default_signature_path(&target));
    fs::write(&out, hex(&signature.to_bytes()))
        .map_err(|err| format!("could not write {out}: {err}"))?;
    println!("signed {} bytes of {target}", payload.len());
    println!("signature: {out}");
    println!("public:    {}", hex(key.verifying_key().as_bytes()));
    Ok(())
}

fn verify(args: &[String]) -> Result<(), String> {
    let public = flag(args, "--pub").ok_or("verify needs --pub <hex>")?;
    let mut positionals = args
        .iter()
        .filter(|arg| !arg.starts_with("--") && arg.as_str() != public.as_str());
    let target = positionals.next().ok_or("verify needs a file")?;
    let signature_path = positionals.next().ok_or("verify needs a signature file")?;

    let key_bytes = unhex(&public)?;
    let key: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "public key is not 32 bytes".to_string())?;
    let verifying = VerifyingKey::from_bytes(&key).map_err(|err| err.to_string())?;

    let payload = fs::read(target).map_err(|err| format!("could not read {target}: {err}"))?;
    let signature_text = fs::read_to_string(signature_path)
        .map_err(|err| format!("could not read {signature_path}: {err}"))?;
    let signature_bytes = unhex(signature_text.trim())?;
    let signature: [u8; 64] = signature_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "signature is not 64 bytes".to_string())?;

    verifying
        .verify(&payload, &ed25519_dalek::Signature::from_bytes(&signature))
        .map_err(|err| format!("signature did not verify: {err}"))?;
    println!("signature verifies");
    Ok(())
}

fn default_signature_path(target: &str) -> String {
    let path = Path::new(target);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join("manifest.sig").to_string_lossy().into_owned()
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let position = args.iter().position(|arg| arg == name)?;
    args.get(position + 1).cloned()
}

fn positional(args: &[String]) -> Option<String> {
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with("--") {
            skip_next = matches!(arg.as_str(), "--key" | "--out" | "--pub");
            continue;
        }
        return Some(arg.clone());
    }
    None
}

/// Operating-system randomness, without a dependency.
///
/// The tool has exactly one use for randomness - generating a release key on an
/// offline machine - and pulling a crate in for it would put a supply-chain
/// dependency inside the one code path whose whole job is to be trustworthy.
#[cfg(windows)]
fn getrandom(_buffer: &mut [u8; 32]) -> Result<(), String> {
    // Windows randomness means an FFI declaration this workspace forbids. Release
    // keys are generated on the offline signing machine anyway, which runs Linux;
    // see `docs/runbooks/adding-a-model.md`.
    Err("generate release keys on the offline signing machine, not on Windows".to_string())
}

#[cfg(not(windows))]
fn getrandom(buffer: &mut [u8; 32]) -> Result<(), String> {
    use std::io::Read;

    let mut file =
        fs::File::open("/dev/urandom").map_err(|err| format!("no system randomness: {err}"))?;
    file.read_exact(buffer)
        .map_err(|err| format!("could not read randomness: {err}"))?;
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return Err(format!("hex string of odd length {}", text.len()));
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    for pair in bytes.chunks_exact(2) {
        let text = std::str::from_utf8(pair).map_err(|err| err.to_string())?;
        out.push(u8::from_str_radix(text, 16).map_err(|err| err.to_string())?);
    }
    Ok(out)
}
