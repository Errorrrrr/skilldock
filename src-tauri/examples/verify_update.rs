use base64::{Engine, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};
use std::{env, fs};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(args.next().ok_or("missing config")?)?)?;
    let encoded = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("missing public key")?;
    let key_text = String::from_utf8(STANDARD.decode(encoded.trim())?)?;
    let key = PublicKey::decode(&key_text)?;
    let signatures: Vec<_> = args.collect();
    if signatures.is_empty() {
        return Err("missing signatures".into());
    }
    for path in signatures {
        let signature_text =
            String::from_utf8(STANDARD.decode(fs::read_to_string(&path)?.trim())?)?;
        let signature = Signature::decode(&signature_text)?;
        let artifact = path.strip_suffix(".sig").ok_or("invalid signature path")?;
        key.verify(&fs::read(artifact)?, &signature, true)?;
        println!("Verified updater artifact: {artifact}");
    }
    Ok(())
}
