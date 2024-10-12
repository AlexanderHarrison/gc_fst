use gc_fst::*;

const HELP: &'static str = 
"Usage: gc_fst extract <iso path>
       gc_fst rebuild <root path> [iso path]
       gc_fst set-header <ISO.hdr path | iso path> <game ID> [game title]

       gc_fst read <iso path> [ <path in iso> <path to file> ] * n

       gc_fst fs <iso path> [
           insert <path in iso> <path to file>
           delete <path in iso>
       ] * n";

fn usage() -> ! {
    eprintln!("{}", HELP);
    std::process::exit(1);
}

macro_rules! unwrap_usage {
    ($e:expr) => {
        match $e {
            Some(e) => e,
            None => usage(),
        }
    }
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(|s| s.as_str()) {
        Some("read") => {
            let iso = unwrap_usage!(args.get(2).map(|s| s.as_str()));
            let mut files = Vec::with_capacity(args[3..].len() / 2);

            let mut i = 3;
            while i < args.len() {
                let iso_path = std::path::Path::new(&args[i]);
                let read_path = std::path::Path::new(unwrap_usage!(args.get(i+1)));
                files.push((iso_path, read_path));
                i += 2;
            }

            match read_iso_files(std::path::Path::new(iso), &files) {
                Ok(()) => {},
                Err(ReadISOFilesError::IOError(e)) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                },
                Err(ReadISOFilesError::InvalidISO) => {
                    eprintln!("Error: file is not an iso or is corrupted");
                    std::process::exit(1);
                },
                Err(ReadISOFilesError::InvalidFSPath(path)) => {
                    eprintln!("Error: file path '{}' does not exist", path.display());
                    std::process::exit(1);
                }
            }
        }
        Some("fs") => {
            let iso = unwrap_usage!(args.get(2).map(|s| s.as_str()));

            let mut cmds = Vec::with_capacity(args[3..].len() / 2);

            let mut i = 3;
            while i < args.len() {
                match args[i].as_str() {
                    "insert" => {
                        cmds.push(IsoOp::Insert {
                            iso_path: std::path::Path::new(unwrap_usage!(args.get(i+1))),
                            input_path: std::path::Path::new(unwrap_usage!(args.get(i+2))),
                        });
                        i += 3;
                    },
                    "delete" => {
                        cmds.push(IsoOp::Delete {
                            iso_path: std::path::Path::new(unwrap_usage!(args.get(i+1))),
                        });
                        i += 2;
                    }
                    _ => usage()
                }
            }

            match operate_on_iso(std::path::Path::new(iso), &cmds) {
                Ok(_) => (),
                Err(OperateISOError::IOError(e)) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                },
                Err(OperateISOError::FileInsertionReplicatesFolder(path)) => {
                    eprintln!("Error: insertion path '{}' already exists as a folder", path.display());
                    std::process::exit(1);
                }
                Err(OperateISOError::InvalidISOPath(path)) => {
                    eprintln!("Error: iso path '{}' does not exist", path.display());
                    std::process::exit(1);
                }
                Err(OperateISOError::InvalidFSPath(path)) => {
                    eprintln!("Error: file path '{}' does not exist", path.display());
                    std::process::exit(1);
                }
                Err(OperateISOError::InvalidISO) => {
                    eprintln!("Error: file is not an iso or is corrupted");
                    std::process::exit(1);
                }
                Err(OperateISOError::TOCTooLarge) => {
                    eprintln!("Error: table of contents is too large, too many files added.");
                    std::process::exit(1);
                }
                Err(OperateISOError::ISOTooLarge) => {
                    eprintln!("Error: resulting ISO is too large, too many files added.");
                    std::process::exit(1);
                }
            }
        }

        Some("set-header") => {
            let path = unwrap_usage!(args.get(2).map(|s| s.as_str()));
            let game_id = unwrap_usage!(args.get(3).map(|s| s.as_str()));

            let mut f = match std::fs::File::options().write(true).open(path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Error: Could not open file '{}'", e);
                    std::process::exit(1);
                }
            };

            if game_id.len() != 6 
                || game_id[0..4].chars().any(|c| !c.is_ascii_uppercase()) 
                || game_id[4..6].chars().any(|c| !c.is_ascii_digit()) 
            {
                eprintln!("Error: Invalid game ID: '{}'. Expected ID such as 'GALE01'", game_id);
                std::process::exit(1);
            }

            use std::io::{Seek, Write};
            if let Err(e) = f.write_all(game_id.as_bytes()) {
                eprintln!("Error: Could not write file '{}'", e);
                std::process::exit(1);
            }

            match args.get(4).map(|s| s.as_str()) {
                Some(title) if title.len() >= 0x20 => {
                    eprintln!("Error: game title is too long");
                    std::process::exit(1);
                }
                Some(title) => {
                    let mut bytes = [0u8; 0x20];
                    bytes[0..title.len()].copy_from_slice(title.as_bytes());

                    if let Err(e) = f.seek(std::io::SeekFrom::Start(0x20)) {
                        eprintln!("Error: Could not seek file '{}'", e);
                        std::process::exit(1);
                    }

                    if let Err(e) = f.write_all(&bytes) {
                        eprintln!("Error: Could not write file '{}'", e);
                        std::process::exit(1);
                    }
                },
                None => (),
            };
        }
        Some("extract") => {
            let iso_path = unwrap_usage!(args.get(2).map(|s| s.as_str()));

            let iso = match std::fs::read(&iso_path) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("Error: Could not read iso '{}'", e);
                    std::process::exit(1);
                }
            };

            match read_iso(&iso) {
                Ok(_) => (),
                Err(ReadISOError::RootDirNotEmpty) => {
                    eprintln!("Error: root directory is not empty");
                    std::process::exit(1);
                }
                Err(ReadISOError::InvalidISO) => {
                    eprintln!("Error: iso path does not exist");
                    std::process::exit(1);
                },
                Err(ReadISOError::WriteFileError(e)) => {
                    eprintln!("Error: Could not write file '{}'", e);
                    std::process::exit(1);
                },
                Err(ReadISOError::CreateDirError(e)) => {
                    eprintln!("Error: Could not create directory '{}'", e);
                    std::process::exit(1);
                },
            }
        }
        Some("rebuild") => {
            let root_path = unwrap_usage!(args.get(2).map(|s| s.as_str()));

            let iso_path = match args.get(3).map(|s| s.as_str()) {
                Some(p) => p,
                None => "out.iso",
            };

            let bytes = match write_iso(std::path::Path::new(root_path)) {
                Ok(b) => b,
                Err(WriteISOError::ISOTooLarge) => {
                    eprintln!("Error: Resulting ISO is too large");
                    std::process::exit(1);
                },
                Err(WriteISOError::InvalidFilename(f)) => {
                    eprintln!("Error: Filename '{:?}' cannot be written in an ISO", f);
                    std::process::exit(1);
                },
                Err(WriteISOError::ReadFileError(e)) => {
                    eprintln!("Error: Could not read file '{}'", e);
                    std::process::exit(1);
                },
                Err(WriteISOError::ReadDirError(e)) => {
                    eprintln!("Error: Could not read directory '{}'", e);
                    std::process::exit(1);
                },
            };

            std::fs::write(&iso_path, &bytes).unwrap();
        }
        _ => usage(),
    }
}
