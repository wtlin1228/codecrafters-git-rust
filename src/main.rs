use clap::{Parser, Subcommand};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use sha1::{Digest, Sha1};
use std::fs;
use std::io::{self, Read, Write};

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

    HashObject {
        /// Actually write the object into the object database.
        #[arg(short)]
        w: bool,

        file: String,
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
                let content = decode(&compressed_content[..]).unwrap();
                print!("{}", content.split('\0').nth(1).unwrap());
            }
        }
        Command::HashObject { w, file } => {
            // prepare git object
            let mut content = fs::read(file).unwrap();
            let mut git_object_formatted_content = format!("blob {}\0", content.len()).into_bytes();
            git_object_formatted_content.append(&mut content);

            // do hash
            let mut hasher = Sha1::new();
            hasher.update(&git_object_formatted_content[..]);
            let sha_hash = format!("{:x}", hasher.finalize());
            println!("{}", sha_hash);

            // write to .git/objects/ if -w is presented
            if w {
                let target_dir = format!(".git/objects/{}", &sha_hash[..2]);
                if fs::read_dir(target_dir.as_str()).is_err() {
                    fs::create_dir(target_dir.as_str()).unwrap();
                };
                fs::write(
                    format!("{}/{}", target_dir, &sha_hash[2..]),
                    encode(&git_object_formatted_content[..]).unwrap(),
                )
                .unwrap();
            }
        }
    }
}

fn decode(bytes: &[u8]) -> io::Result<String> {
    let mut z = ZlibDecoder::new(bytes);
    let mut s = String::new();
    z.read_to_string(&mut s)?;
    Ok(s)
}

fn encode(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(bytes).unwrap();
    e.finish()
}
