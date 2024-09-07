const HEADER_INFO_OFFSET: u32 = 0x420;

#[derive(Debug)]
pub enum ReadISOError {
    InvalidISO,
    RootDirNotEmpty,
    WriteFileError(std::io::Error),
    CreateDirError(std::io::Error),
}

#[derive(Debug)]
pub enum WriteISOError {
    ISOTooLarge,
    InvalidFilename(std::ffi::OsString),
    ReadFileError(std::io::Error),
    ReadDirError(std::io::Error),
}

pub const ROM_SIZE: u32 = 0x57058000;

pub fn write_iso(root: &std::path::Path) -> Result<Vec<u8>, WriteISOError> {
    const SEGMENT_ALIGNMENT: u32 = 8;

    let mut iso = Vec::with_capacity(ROM_SIZE as usize);
    let mut path = root.to_path_buf();
    
    // write special files -------------------------------------------------

    path.push("&&systemdata");


    path.push("ISO.hdr");
    {
        let mut header_file = std::fs::File::open(&path).map_err(|e| WriteISOError::ReadFileError(e))?;
        std::io::copy(&mut header_file, &mut iso).map_err(|e| WriteISOError::ReadFileError(e))?;
    }
    path.pop();
    // overwritten later: dol_offset, fst_offset, fst_size, max_fst_size @ 0x420


    path.push("AppLoader.ldr");
    {
        let mut apploader_file = std::fs::File::open(&path).map_err(|e| WriteISOError::ReadFileError(e))?;
        std::io::copy(&mut apploader_file, &mut iso).map_err(|e| WriteISOError::ReadFileError(e))?;
    }
    path.pop();


    let rounded_size = align(iso.len() as u32, SEGMENT_ALIGNMENT);
    iso.resize(rounded_size as usize, 0u8);


    path.push("Start.dol");
    let dol_offset = iso.len() as u32;
    {
        let mut dol_file = std::fs::File::open(&path).map_err(|e| WriteISOError::ReadFileError(e))?;
        std::io::copy(&mut dol_file, &mut iso).map_err(|e| WriteISOError::ReadFileError(e))?;
    }
    path.pop();


    let rounded_size = align(iso.len() as u32, SEGMENT_ALIGNMENT);
    iso.resize(rounded_size as usize, 0u8);

    // pop &&systemdata
    path.pop();

    // write filesystem header, string table, and contents ---------------------------------------

    let fst_offset = iso.len() as u32;

    // we need the number of entries before we can write the strings, so we do a lil prepass.
    let (entry_count, total_string_length) = count_entries(&path)?;
    iso.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
    // entry_count technically includes this header, so we add 1 to it.
    iso.extend_from_slice(&(entry_count+1).to_be_bytes());

    let fs_end = fst_offset + 0xC*(entry_count+1);
    let string_start = fs_end;
    let string_end = string_start + total_string_length;
    let fs_size = string_end - fst_offset;
    iso.resize(string_end as usize, 0u8);

    write_u32(&mut iso, HEADER_INFO_OFFSET+0, dol_offset);
    write_u32(&mut iso, HEADER_INFO_OFFSET+4, fst_offset);
    write_u32(&mut iso, HEADER_INFO_OFFSET+8, fs_size);
    write_u32(&mut iso, HEADER_INFO_OFFSET+12, fs_size);

    let entry_start = fst_offset + 0xC;
    let mut entry_offset = entry_start;
    let mut string_offset = string_start;

    write_dir(
        &mut path, 
        &mut iso,
        0,
        entry_start,
        &mut entry_offset,
        string_start,
        &mut string_offset,
    )?;
    
    if iso.len() > ROM_SIZE as usize { return Err(WriteISOError::ISOTooLarge); }
    iso.resize(ROM_SIZE as usize, 0u8);

    Ok(iso)
}

/// recursively called for each dir in root
fn write_dir(
    path: &std::path::Path, 
    iso: &mut Vec<u8>,
    parent_dir_idx: u32,
    entry_start: u32,
    entry_offset: &mut u32, 
    string_start: u32,
    string_offset: &mut u32, 
) -> Result<(), WriteISOError> {
    const FILE_CONTENTS_ALIGNMENT: u32 = 15; // 32k
    let mut path = path.to_path_buf();

    struct Entry {
        pub name: String,
        pub size: Option<u32>,
    }

    let mut entries = Vec::with_capacity(256);

    for entry in std::fs::read_dir(&path).map_err(|e| WriteISOError::ReadDirError(e))? {
        let entry = entry.map_err(|e| WriteISOError::ReadDirError(e))?;
        let metadata = entry.metadata().map_err(|e| WriteISOError::ReadDirError(e))?;
        if metadata.is_file() {
            entries.push(Entry {
                name: entry.file_name().into_string().map_err(|f| WriteISOError::InvalidFilename(f))?,
                size: Some(metadata.len() as u32),
            })
        } else if metadata.is_dir() {
            let dir_name = entry.file_name();
            if dir_name == "&&systemdata" { continue; }

            entries.push(Entry {
                name: dir_name.into_string().map_err(|f| WriteISOError::InvalidFilename(f))?,
                size: None,
            })
        }
    }

    fn cmp_case_insensitive(a: &str, b: &str) -> std::cmp::Ordering {
        a.chars()
            .map(|c| c.to_ascii_lowercase())
            .cmp(b.chars().map(|c| c.to_ascii_lowercase()))
    }

    entries.sort_by(|a, b| cmp_case_insensitive(&a.name, &b.name));

    for Entry { name, size } in entries {
        if let Some(size) = size {
            let rounded_size = align(iso.len() as u32, FILE_CONTENTS_ALIGNMENT);
            iso.resize(rounded_size as usize, 0u8);

            // entry data
            write_u32(iso, *entry_offset, *string_offset - string_start);
            let contents_offset = iso.len() as u32;
            write_u32(iso, *entry_offset+4, contents_offset);
            write_u32(iso, *entry_offset+8, size);
            *entry_offset += 0xC;

            // file name
            let file_name_len = name.len() as u32;
            iso[*string_offset as usize..][..file_name_len as usize].copy_from_slice(name.as_bytes());
            iso[(*string_offset + file_name_len) as usize] = 0; // ensure null terminator
            *string_offset += file_name_len + 1;

            // contents
            path.push(&name);
            let mut file = std::fs::File::open(&path).map_err(|e| WriteISOError::ReadFileError(e))?;
            std::io::copy(&mut file, iso).map_err(|e| WriteISOError::ReadFileError(e))?;
            path.pop();
        } else {
            // entry data
            let string_offset_from_start = *string_offset - string_start;
            let mut w0 = string_offset_from_start.to_be_bytes();
            w0[0] = 1; // directory flag
            iso[*entry_offset as usize..][..4].copy_from_slice(&w0);
            write_u32(iso, *entry_offset+4, parent_dir_idx);
            // next idx written later
            let next_idx_offset = *entry_offset + 8;
            *entry_offset += 0xC;

            // dir name
            let dir_name_len = name.len() as u32;
            iso[*string_offset as usize..][..dir_name_len as usize].copy_from_slice(name.as_bytes());
            iso[(*string_offset + dir_name_len) as usize] = 0; // null terminator
            *string_offset += dir_name_len + 1;

            let sub_dir = path.join(&name);
            let entry_index = (*entry_offset - entry_start) / 0xC; // 1-based index, so compute after 12 byte increment was added.
            write_dir(
                &sub_dir,
                iso,
                entry_index,
                entry_start,
                entry_offset,
                string_start,
                string_offset,
            )?;

            // Add 1 to fix off by one. These indices are a little weird.
            let next_idx = (*entry_offset - entry_start) / 0xC + 1;
            write_u32(iso, next_idx_offset, next_idx);
        }
    }

    Ok(())
}

fn count_entries(path: &std::path::Path) -> Result<(u32, u32), WriteISOError> {
    let mut entry_count = 0;
    let mut total_string_length = 0;

    for entry in std::fs::read_dir(path).map_err(|e| WriteISOError::ReadDirError(e))? {
        let entry = entry.map_err(|e| WriteISOError::ReadDirError(e))?;
        match entry.file_type() {
            Err(e) => return Err(WriteISOError::ReadDirError(e)),
            Ok(f) if f.is_file() => {
                entry_count += 1;
                total_string_length += entry.file_name().len() as u32 + 1;
            }
            Ok(f) if f.is_dir() => {
                let file_name = entry.file_name();
                if file_name == "&&systemdata" { continue; }

                entry_count += 1;
                total_string_length += file_name.len() as u32 + 1;
                // must realloc due to borrowing issues. No big deal cuz we're IO bottlenecked anyways.
                let new_path = path.join(&file_name);
                let (ec, sl) = count_entries(&new_path)?;
                entry_count += ec;
                total_string_length += sl;
            }

            // ignore symlinks
            _ => (),
        }
    }

    Ok((entry_count, total_string_length))
}

pub fn read_iso(iso: &[u8]) -> Result<(), ReadISOError> {
    if iso.len() != ROM_SIZE as usize { return Err(ReadISOError::InvalidISO); }

    let fst_offset = read_u32(iso, HEADER_INFO_OFFSET+4);
    let entry_count = read_u32(iso, fst_offset + 0x8);
    let string_table_offset = fst_offset + entry_count * 0xC;
    let entry_start_offset = fst_offset + 0xC;

    // write regular files ---------------------------------------------------

    let mut path = std::path::PathBuf::from("./root/");
    
    if std::fs::read_dir(&path).is_ok_and(|p| p.count() != 0) {
        return Err(ReadISOError::RootDirNotEmpty);
    }
    std::fs::create_dir_all(&path).map_err(|e| ReadISOError::CreateDirError(e))?;

    let mut dir_end_indices = Vec::with_capacity(8);
    let mut offset = entry_start_offset;
    let mut entry_index = 1;
    while offset < string_table_offset {
        while Some(entry_index) == dir_end_indices.last().copied() {
            // dir has ended
            dir_end_indices.pop();
            path.pop();
        }

        let is_file = iso[offset as usize] == 0;

        let mut filename_offset_buf = [0; 4];
        filename_offset_buf[1] = iso[offset as usize+1];
        filename_offset_buf[2] = iso[offset as usize+2];
        filename_offset_buf[3] = iso[offset as usize+3];
        let filename_offset = u32::from_be_bytes(filename_offset_buf);
        let filename = read_filename(iso, string_table_offset + filename_offset)
            .ok_or(ReadISOError::InvalidISO)?;

        if is_file {
            let file_offset = read_u32(iso, offset+4);
            let file_size = read_u32(iso, offset+8);

            path.push(filename);
            std::fs::write(&path, &iso[file_offset as usize..][..file_size as usize])
                .map_err(|e| ReadISOError::WriteFileError(e))?;
            path.pop();
        } else {
            //let parent_idx = read_u32(iso, offset+4); // unused
            let next_idx = read_u32(iso, offset+8);
            dir_end_indices.push(next_idx);
            path.push(filename);
            std::fs::create_dir_all(&path).map_err(|e| ReadISOError::CreateDirError(e))?;
        }

        offset += 0xC;
        entry_index += 1;
    }

    // write special (&&systemdata) files ------------------------------------

    path.clear();
    path.push("./root");
    path.push("&&systemdata");
    std::fs::create_dir_all(&path).map_err(|e| ReadISOError::CreateDirError(e))?;

    path.push("ISO.hdr");
    std::fs::write(&path, &iso[0..0x2440])
        .map_err(|e| ReadISOError::WriteFileError(e))?;
    path.pop();

    path.push("AppLoader.ldr");
    let apploader_code_size = read_u32(iso, 0x2454);
    let apploader_trailer_size = read_u32(iso, 0x2458);
    let apploader_total_size = align(apploader_code_size + apploader_trailer_size, 5);
    let apploader_end = 0x2440 + apploader_total_size;
    std::fs::write(&path, &iso[0x2440..apploader_end as usize])
        .map_err(|e| ReadISOError::WriteFileError(e))?;
    path.pop();

    path.push("Start.dol");
    let dol_offset = read_u32(iso, HEADER_INFO_OFFSET);
    let dol_size = (0..18).map(|i| {
        let segment_offset = read_u32(iso, dol_offset+i*4);
        let segment_size = read_u32(iso, dol_offset + 0x90 + i*4);
        segment_offset+segment_size
    }).max().unwrap();
    let dol_end = dol_offset + dol_size;
    std::fs::write(&path, &iso[dol_offset as usize..dol_end as usize])
        .map_err(|e| ReadISOError::WriteFileError(e))?;
    path.pop();

    // We don't write Game.toc. It's pretty much useless.
    // The point of exporting the fs is to modify, add, and remove files,
    // which means we have to recreate the table of contents anyways when rebuilding the iso.

    Ok(())
}

fn read_u32(iso: &[u8], offset: u32) -> u32 {
    u32::from_be_bytes(iso[offset as usize..][..4].try_into().unwrap())
}

fn write_u32(iso: &mut Vec<u8>, offset: u32, n: u32) {
    iso[offset as usize..][..4].copy_from_slice(&n.to_be_bytes());
}

fn read_filename(iso: &[u8], offset: u32) -> Option<&str> {
    std::ffi::CStr::from_bytes_until_nul(&iso[offset as usize..]).ok()?.to_str().ok()
}

/// rounds up to nearest multiple of 1<<bits
fn align(n: u32, bits: u32) -> u32 { 
    let mask = (1 << bits) - 1;
    (n + mask) & !mask
}
