use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;
use std::fs;
use std::io;
use std::io::prelude::*;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Init,
    CatFile {
        /// Pretty-print the contents of <object> based on its type.
        #[arg(short)]
        pretty_print: bool,

        /// The name of the object to show. For a more complete list of ways to spell object names, see the
        /// "SPECIFYING REVISIONS" section in gitrevisions(7).
        object: String,
    },
}

fn main() {
    match Args::parse().command {
        Command::Init => {
            fs::create_dir(".git").unwrap();
            fs::create_dir(".git/objects").unwrap();
            fs::create_dir(".git/refs").unwrap();
            fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
            println!("Initialized git directory")
        }
        Command::CatFile {
            pretty_print,
            object,
        } => {
            let (folder, filename) = object.split_at(2);
            let compressed_content =
                fs::read(format!(".git/objects/{}/{}", folder, filename)).unwrap();
            if pretty_print {
                let content = decode_reader(compressed_content).unwrap();
                print!("{}", content.split('\0').nth(1).unwrap());
            }
        }
    }
}

fn decode_reader(bytes: Vec<u8>) -> io::Result<String> {
    let mut z = ZlibDecoder::new(&bytes[..]);
    let mut s = String::new();
    z.read_to_string(&mut s)?;
    Ok(s)
}
