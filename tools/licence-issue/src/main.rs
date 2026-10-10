//! `licence-issue` - signs AURA licence keys. ADR-0105.
//!
//! The vendor key is 32 random bytes in a file that lives offline and is backed up, never in this
//! repository or on a CI runner. Create it once with the operating system's randomness:
//!
//! ```text
//! python -c "import secrets; open('vendor-licence.key', 'wb').write(secrets.token_bytes(32))"
//! ```
//!
//! ```text
//! licence-issue public --key vendor-licence.key
//!     prints the public key as the Rust array `aura_licence::VENDOR_PUBLIC_KEY` must hold
//! licence-issue issue --key vendor-licence.key --name "Asha Studio" --email asha@example.com
//!     [--edition pro] [--id order-1001] [--expires 2027-10-07] [--issued 2026-10-08]
//!     prints the key to send to the customer
//! licence-issue verify <key>
//!     checks a key against the public key AURA ships and prints what it grants (for support)
//! ```
//!
//! A key is checked against the shipped public key before it is printed, so a key signed with the
//! wrong vendor file is caught here rather than by a customer.
//!
//! Panics are permitted in this crate: it is an operator tool run by a human at a terminal.

use std::env;
use std::fs;
use std::process::ExitCode;

use aura_licence::Licence;
use ed25519_dalek::{SigningKey, VerifyingKey};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("public") => public(&args[1..]),
        Some("issue") => issue(&args[1..]),
        Some("verify") => verify(&args[1..]),
        _ => Err("usage:\n  licence-issue public --key <file>\n  licence-issue verify <key>\n  licence-issue issue --key <file> --name <name> --email <email> [--edition pro] [--id <ref>] [--expires YYYY-MM-DD] [--issued YYYY-MM-DD]".to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("licence-issue: {message}");
            ExitCode::FAILURE
        }
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn signing_key(args: &[String]) -> Result<SigningKey, String> {
    let path = flag(args, "--key").ok_or("--key <file> is required")?;
    let bytes = fs::read(&path).map_err(|e| format!("could not read {path}: {e}"))?;
    let seed: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| format!("{path} is not a 32-byte key"))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn public(args: &[String]) -> Result<(), String> {
    let key = signing_key(args)?;
    let bytes = key.verifying_key().to_bytes();
    let lines: Vec<String> = bytes
        .chunks(16)
        .map(|row| {
            let cells: Vec<String> = row.iter().map(|b| format!("0x{b:02x}")).collect();
            format!("    {},", cells.join(", "))
        })
        .collect();
    println!(
        "pub const VENDOR_PUBLIC_KEY: [u8; 32] = [\n{}\n];",
        lines.join("\n")
    );
    Ok(())
}

fn issue(args: &[String]) -> Result<(), String> {
    let key = signing_key(args)?;
    let today = time::OffsetDateTime::now_utc().date();
    let issued = flag(args, "--issued").unwrap_or_else(|| {
        format!(
            "{:04}-{:02}-{:02}",
            today.year(),
            u8::from(today.month()),
            today.day()
        )
    });
    let licence = Licence {
        id: flag(args, "--id").unwrap_or_default(),
        name: flag(args, "--name").ok_or("--name is required")?,
        email: flag(args, "--email").ok_or("--email is required")?,
        edition: flag(args, "--edition").unwrap_or_else(|| "pro".into()),
        issued,
        expires: flag(args, "--expires"),
    };
    let text = aura_licence::encode(&licence, &key).map_err(|e| e.to_string())?;
    let shipped =
        VerifyingKey::from_bytes(&aura_licence::VENDOR_PUBLIC_KEY).map_err(|e| e.to_string())?;
    if shipped != key.verifying_key() {
        return Err(
            "this key file is not the one whose public half AURA ships; the customer's copy would refuse the key"
                .into(),
        );
    }
    aura_licence::decode(&text).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(())
}

fn verify(args: &[String]) -> Result<(), String> {
    let key = args.first().ok_or("verify needs the key")?;
    let licence = aura_licence::decode(key).map_err(|e| e.to_string())?;
    println!("valid AURA key");
    println!("  id:      {}", licence.id);
    println!("  name:    {}", licence.name);
    println!("  email:   {}", licence.email);
    println!("  edition: {}", licence.edition);
    println!("  issued:  {}", licence.issued);
    println!(
        "  expires: {}",
        licence.expires.as_deref().unwrap_or("never")
    );
    Ok(())
}
