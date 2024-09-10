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

#[derive(Debug)]
#[cfg(feature = "png")]
pub enum FromPngError {
    DecodeError(lodepng::Error),
}

pub const ROM_SIZE: u32 = 0x57058000;

#[derive(Clone, Debug)]
pub struct RGB5A1Image(pub Box<[u8; 0x1800]>);

impl RGB5A1Image {
    /// Convert from an rgba8 image. 
    ///
    /// Expects a 96*32 image in rows of pixels.
    /// 
    /// Pixels with an alpha greater or equal to 128 will be converted to fully transparent pixels in the file.
    /// Pixels with lesser alpha values will be turned opaque.
    pub fn from_rgba8(data: &[[u8; 4]; 96*32]) -> Self {
        let mut out = Box::new([0u8; 0x1800]);

        const TILES_X: usize = 24;
        const TILES_Y: usize = 8;

        let mut out_i = 0;
        for tile_y in 0..TILES_Y {
            for tile_x in 0..TILES_X {
                for ty in 0..4 {
                    for tx in 0..4 {
                        let y = tile_y*4 + ty;
                        let x = tile_x*4 + tx;
                        let in_i = x + y*96;

                        let [r, g, b, a] = data[in_i];

                        let new_r = r >> 3;
                        let new_g = g >> 3;
                        let new_b = b >> 3;
                        let new_a = a >> 7;

                        let b1 = (new_a << 7) ^ (new_r << 2) ^ (new_g >> 3);
                        let b2 = (new_g << 5) ^ new_b;

                        out[out_i] = b1;
                        out[out_i+1] = b2;

                        out_i += 2;
                    }
                }
            }
        }

        Self(out)
    }
}


#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GameRegion { UsOrJp, Eu, }

#[derive(Copy, Clone, Debug)]
pub struct GameInfo<'a> {
    pub region: GameRegion,

    /// Must be less than 0x20 bytes.
    pub game_title: &'a str,

    /// Must be less than 0x20 bytes.
    pub developer_title: &'a str,

    /// Must be less than 0x40 bytes.
    pub full_game_title: &'a str,

    /// Must be less than 0x40 bytes.
    pub full_developer_title: &'a str,

    /// Must be less than 0x80 bytes.
    pub game_description: &'a str,

    pub banner: &'a RGB5A1Image,
}

#[derive(Copy, Clone, Debug)]
pub enum CreateOpeningBnrError {
    GameTitleTooLong,
    DevTitleTooLong,
    FullGameTitleTooLong,
    FullDevTitleTooLong,
    GameDescTooLong,
}

impl<'a> GameInfo<'a> {
    pub fn verify(&self) -> Result<(), CreateOpeningBnrError> {
        if self.game_title.len()                >= 0x20 { return Err(CreateOpeningBnrError::GameTitleTooLong)     }
        else if self.developer_title.len()      >= 0x20 { return Err(CreateOpeningBnrError::DevTitleTooLong)      }
        else if self.full_game_title.len()      >= 0x40 { return Err(CreateOpeningBnrError::FullGameTitleTooLong) }
        else if self.full_developer_title.len() >= 0x40 { return Err(CreateOpeningBnrError::FullDevTitleTooLong)  }
        else if self.game_description.len()     >= 0x80 { return Err(CreateOpeningBnrError::GameDescTooLong)      }

        Ok(())
    }
}

/// Converts fields into an 'opening.bnr' file.
pub fn create_opening_bnr(info: GameInfo) -> Result<Box<[u8; 0x1960]>, CreateOpeningBnrError> {
    info.verify()?;

    let mut file = Box::new([0u8; 0x1960]);
    let region = match info.region {
        GameRegion::UsOrJp => b"BNR1",
        GameRegion::Eu => b"BNR2",
    };
    file[0..4].copy_from_slice(region);
    file[0x20..][..0x1800].copy_from_slice(&*info.banner.0);
    file[0x1820..][..info.game_title.len()].copy_from_slice(info.game_title.as_bytes());
    file[0x1840..][..info.developer_title.len()].copy_from_slice(info.developer_title.as_bytes());
    file[0x1860..][..info.full_game_title.len()].copy_from_slice(info.full_game_title.as_bytes());
    file[0x18A0..][..info.full_developer_title.len()].copy_from_slice(info.full_developer_title.as_bytes());
    file[0x18E0..][..info.game_description.len()].copy_from_slice(info.game_description.as_bytes());

    Ok(file)
}

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
