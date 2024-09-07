use gc_fst::*;

const HELP: &'static str = 
"Usage: gc_fst extract <iso path>
       gc_fst rebuild <root path> [iso path]";

fn usage() -> ! {
    eprintln!("{}", HELP);
    std::process::exit(1);
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(|s| s.as_str()) {
        Some("extract") => {
            let iso_path = match args.get(2).map(|s| s.as_str()) {
                Some(p) => p,
                None => usage(),
            };

            let iso = match std::fs::read(&iso_path) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("Error: Could not read iso: {}", e);
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
                    eprintln!("Error: Could not write file: {}", e);
                    std::process::exit(1);
                },
                Err(ReadISOError::CreateDirError(e)) => {
                    eprintln!("Error: Could not create directory: {}", e);
                    std::process::exit(1);
                },
            }
        }
        Some("rebuild") => {
            let root_path = match args.get(2).map(|s| s.as_str()) {
                Some(p) => p,
                None => usage(),
            };

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
                    eprintln!("Error: Filename {:?} cannot be written in an ISO", f);
                    std::process::exit(1);
                },
                Err(WriteISOError::ReadFileError(e)) => {
                    eprintln!("Error: Could not read file: {}", e);
                    std::process::exit(1);
                },
                Err(WriteISOError::ReadDirError(e)) => {
                    eprintln!("Error: Could not read directory: {}", e);
                    std::process::exit(1);
                },
            };

            std::fs::write(&iso_path, &bytes).unwrap();
        }
        _ => usage(),
    }
}
