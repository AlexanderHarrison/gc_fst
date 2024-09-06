fn main() {
    match std::env::args().nth(1).as_deref().unwrap() {
        "unpack" => {
            let iso_path = std::env::args().nth(2).expect("iso path not passed");
            let iso = std::fs::read(&iso_path).unwrap();
            gc_fst::read_iso(&iso).unwrap();
        }
        "rebuild" => {
            let bytes = gc_fst::write_iso(std::path::Path::new("root")).unwrap();
            std::fs::write("out.iso", &bytes).unwrap();
        }
        s => eprintln!("unk {}", s),
    }
}
