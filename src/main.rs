use anyhow::Context;
use clap::{Parser, Subcommand};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use sha1::{Digest, Sha1};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::io::BufReader;

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
        write: bool,

        file: String,
    },

    LsTree {
        /// list only filenames
        #[arg(long)]
        name_only: bool,

        tree_ish: String,
    },
}

fn main() -> anyhow::Result<()> {
    match Args::parse().command {
        Command::Init => {
            fs::create_dir(".git").unwrap();
            fs::create_dir(".git/objects").unwrap();
            fs::create_dir(".git/refs").unwrap();
            fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
            println!("Initialized git directory");
        }
        Command::CatFile {
            pretty_print,
            object,
        } => {
            anyhow::ensure!(pretty_print, "only support -p");

            let mut object = Object::read(&object)?;
            match object.kind {
                Kind::Blob => {
                    let stdout = io::stdout();
                    let mut handle = stdout.lock();
                    std::io::copy(&mut object.reader, &mut handle)
                        .context("write blob to stdout")?;
                }
                _ => todo!(),
            }
        }
        Command::HashObject { write, file } => {
            // prepare git object
            let mut content = fs::read(file)?;
            let mut git_object_formatted_content = format!("blob {}\0", content.len()).into_bytes();
            git_object_formatted_content.append(&mut content);

            // do hash
            let mut hasher = Sha1::new();
            hasher.update(&git_object_formatted_content[..]);
            let sha_hash = format!("{:x}", hasher.finalize());
            println!("{}", sha_hash);

            // write to .git/objects/ if -w is presented
            if write {
                let target_dir = format!(".git/objects/{}", &sha_hash[..2]);
                if fs::read_dir(target_dir.as_str()).is_err() {
                    fs::create_dir(target_dir.as_str())?;
                };
                fs::write(
                    format!("{}/{}", target_dir, &sha_hash[2..]),
                    encode(&git_object_formatted_content[..])?,
                )?;
            }
        }
        Command::LsTree {
            name_only,
            tree_ish,
        } => {
            let mut object = Object::read(&tree_ish)?;
            match object.kind {
                Kind::Tree => {
                    let stdout = io::stdout();
                    let mut handle = stdout.lock();
                    let mut buf = Vec::new();
                    let mut hashbuf = [0; 20];
                    loop {
                        buf.clear();
                        let n = object
                            .reader
                            .read_until(0, &mut buf)
                            .context("read next tree entry")?;
                        if n == 0 {
                            break;
                        }
                        object
                            .reader
                            .read_exact(&mut hashbuf)
                            .context("read tree entry hash")?;

                        let without_ending_null = buf.split_last().context("split last")?.1;
                        let mut iter = without_ending_null.splitn(2, |&b| b == b' ');
                        let mode = iter.next().context("get tree entry mode")?;
                        let name = iter.next().context("get tree entry name")?;

                        match name_only {
                            true => {
                                handle.write_all(name).context("write name to console")?;
                                write!(handle, "\n").context("write new line to console")?;
                            }
                            false => todo!(),
                        }
                    }
                }
                _ => todo!(),
            }
        }
    }
    Ok(())
}

fn encode(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(bytes).unwrap();
    e.finish()
}

enum Kind {
    Blob,
    Tree,
}

impl Kind {
    fn from(s: &str) -> anyhow::Result<Self> {
        match s {
            "blob" => Ok(Self::Blob),
            "tree" => Ok(Self::Tree),
            _ => anyhow::bail!("invalid kind: {s}"),
        }
    }
}

struct Object<R> {
    kind: Kind,
    expected_size: u64,
    reader: R,
}

impl Object<()> {
    fn read(hash: &str) -> anyhow::Result<Object<impl BufRead>> {
        let (folder, filename) = hash.split_at(2);
        let file_path = format!(".git/objects/{}/{}", folder, filename);
        let f = fs::File::open(&file_path).context(format!("open file: {}", file_path))?;
        let z = ZlibDecoder::new(f);
        let mut reader = BufReader::new(z);
        let mut buf = Vec::new();
        reader
            .read_until(0, &mut buf)
            .context("read header from .git/objects")?;
        let without_ending_null = buf.split_last().context("split last")?.1;
        let header = String::from_utf8(without_ending_null.to_vec())
            .context("object header isn't valid UTF-8")?;
        let (kind, size) = header.split_once(' ').context(format!(
            "object header isn't `<kink> <size>\0`: {:?}",
            header
        ))?;
        let size = size
            .parse::<u64>()
            .context(format!("object header has invalid size: {}", size))?;
        Ok(Object {
            kind: Kind::from(kind)?,
            expected_size: size,
            reader: reader.take(size),
        })
    }
}
