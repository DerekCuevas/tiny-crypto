use base64::Engine;
use clap::{Parser, Subcommand, ValueEnum};
use sha2::{Digest, Sha256};
use strum_macros::Display;

#[derive(Parser)]
#[command(name = "tiny-crypto")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Default, Debug, Clone, Copy, ValueEnum, Display)]
enum ByteDisplay {
    #[default]
    #[strum(to_string = "hex")]
    Hex,
    #[strum(to_string = "base64")]
    Base64,
}

#[derive(Subcommand)]
enum Commands {
    /// Hash a string using SHA-256
    Hash {
        /// The string to hash
        #[arg(short, long)]
        input: String,

        /// Output format (hex, base64)
        #[arg(short, long, default_value_t = ByteDisplay::Hex)]
        format: ByteDisplay,
    },
}

fn hash_string(input: &str, format: ByteDisplay) {
    let hash = Sha256::digest(input.as_bytes());

    match format {
        ByteDisplay::Hex => println!("Hash (hex): {:x}", hash),
        ByteDisplay::Base64 => {
            let bytes = hash.to_vec();
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            println!("Hash (base64): {}", encoded);
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Hash { input, format } => {
            hash_string(&input, format);
        }
    }
}
