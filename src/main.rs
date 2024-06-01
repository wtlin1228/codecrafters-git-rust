use anyhow::Context;
use clap::{Parser, Subcommand};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use sha1::{Digest, Sha1};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;

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

    WriteTree,
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
            anyhow::ensure!(write, "only support -w");

            // prepare git object
            let mut content = fs::read(file)?;
            let mut git_object_formatted_content = format!("blob {}\0", content.len()).into_bytes();
            git_object_formatted_content.append(&mut content);

            let hash = Object::write(&git_object_formatted_content)?;
            println!("{}", hex::encode(hash));
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
                        let _mode = iter.next().context("get tree entry mode")?;
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
        Command::WriteTree => {
            let path = Path::new(".").to_path_buf();
            let hash = write_tree(&path)?;
            if let Some(hash) = hash {
                let hash_hex = hex::encode(hash);
                println!("{}", hash_hex);
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
    #[allow(dead_code)]
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

    fn write(content: &[u8]) -> anyhow::Result<Vec<u8>> {
        // do hash
        let mut hasher = Sha1::new();
        hasher.update(&content[..]);
        let hash = hasher.finalize();
        let hash_hex = hex::encode(hash);

        // write git object
        let target_dir = format!(".git/objects/{}", &hash_hex[..2]);
        fs::create_dir_all(target_dir.as_str())?;
        fs::write(
            format!("{}/{}", target_dir, &hash_hex[2..]),
            encode(&content[..])?,
        )?;

        Ok(hash.to_vec())
    }
}

fn write_tree(path: &PathBuf) -> anyhow::Result<Option<Vec<u8>>> {
    anyhow::ensure!(path.is_dir(), "write to tree in path: {:?}", path);

    let dir = path.read_dir().unwrap();
    let mut entries = dir.fold(Vec::new(), |mut acc, x| match x {
        Ok(entry) => {
            if entry.file_name() != ".git" {
                acc.push(entry);
            }
            acc
        }
        Err(_) => acc,
    });
    entries.sort_by(|a, b| a.file_name().partial_cmp(&b.file_name()).unwrap());

    let mut tree_content: Vec<u8> = Vec::new();
    for entry in entries {
        let file_name = entry.file_name();
        let entry_path = entry.path();
        match entry_path.is_dir() {
            true => {
                let hash = write_tree(&entry_path)
                    .context(format!("write tree on path: {:?}", entry_path))?;

                // append tree entry
                if let Some(hash) = hash {
                    tree_content.extend(b"40000 ");
                    tree_content.extend(file_name.as_encoded_bytes());
                    tree_content.push(0);
                    tree_content.extend(hash);
                }
            }
            false => {
                // prepare git object
                let mut content = fs::read(&entry_path)?;
                let mut git_object_formatted_content =
                    format!("blob {}\0", content.len()).into_bytes();
                git_object_formatted_content.append(&mut content);
                let hash = Object::write(&git_object_formatted_content)?;

                // append tree entry
                tree_content.extend(b"100644 ");
                tree_content.extend(file_name.as_encoded_bytes());
                tree_content.push(0);
                tree_content.extend(hash);
            }
        }
    }

    if tree_content.len() == 0 {
        return Ok(None);
    }

    // prepare git object
    let mut git_object_formatted_content = format!("tree {}\0", tree_content.len()).into_bytes();
    git_object_formatted_content.append(&mut tree_content);
    let hash = Object::write(&git_object_formatted_content)?;

    Ok(Some(hash))
}
