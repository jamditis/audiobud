use minisign_verify::{PublicKey, Signature};
use std::{
    env,
    error::Error,
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
};

const BUFFER_SIZE: usize = 64 * 1024;

fn main() -> Result<(), Box<dyn Error>> {
    let [public_key_path, signature_path, archive_path] = parse_paths()?;
    let public_key = PublicKey::from_file(public_key_path)?;
    let signature = Signature::from_file(signature_path)?;
    let archive = File::open(archive_path)?;
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, archive);
    let mut verifier = public_key.verify_stream(&signature)?;
    let mut buffer = [0_u8; BUFFER_SIZE];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        verifier.update(&buffer[..bytes_read]);
    }

    verifier.finalize()?;
    println!("Updater signature verified");
    Ok(())
}

fn parse_paths() -> Result<[PathBuf; 3], Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let paths = [
        args.next().ok_or("missing public key path")?.into(),
        args.next().ok_or("missing signature path")?.into(),
        args.next().ok_or("missing archive path")?.into(),
    ];
    if args.next().is_some() {
        return Err("expected public key, signature, and archive paths".into());
    }
    Ok(paths)
}
