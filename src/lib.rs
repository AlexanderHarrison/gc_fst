const HEADER_INFO_OFFSET: u32 = 0x420;
const FILE_CONTENTS_ALIGNMENT: u32 = 15; // 32k
const SEGMENT_ALIGNMENT: u32 = 8;

pub const ROM_SIZE: u32 = 0x57058000;

use std::path::{Path, PathBuf};

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
pub enum OperateISOError {
    IOError(std::io::Error),
    OpenError { path: PathBuf, e: std::io::Error },
    FileInsertionReplicatesFolder(PathBuf),
    InvalidISOPath(PathBuf),
    InvalidFSPath(PathBuf),
    InvalidISO,
    TOCTooLarge,
    ISOTooLarge,
}

#[derive(Debug)]
pub enum ReadISOFilesError {
    IOError(std::io::Error),
    InvalidISO,
    InvalidFSPath(PathBuf),
}

impl From<std::io::Error> for OperateISOError {
    fn from(e: std::io::Error) -> Self { OperateISOError::IOError(e) }
}

impl From<std::io::Error> for ReadISOFilesError {
    fn from(e: std::io::Error) -> Self { ReadISOFilesError::IOError(e) }
}

#[derive(Debug)]
#[cfg(feature = "png")]
pub enum FromPngError {
    DecodeError(lodepng::Error),
}

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

pub fn write_iso(root: &Path) -> Result<Vec<u8>, WriteISOError> {
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
    
    // mex makes the iso smaller, so apparently that's alright.
    if iso.len() > ROM_SIZE as usize { return Err(WriteISOError::ISOTooLarge); }
    iso.resize(ROM_SIZE as usize, 0u8);

    Ok(iso)
}

/// recursively called for each dir in root
fn write_dir(
    path: &Path, 
    iso: &mut Vec<u8>,
    parent_dir_idx: u32,
    entry_start: u32,
    entry_offset: &mut u32, 
    string_start: u32,
    string_offset: &mut u32, 
) -> Result<(), WriteISOError> {
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

fn count_entries(path: &Path) -> Result<(u32, u32), WriteISOError> {
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
    // mex makes the iso smaller, so apparently that's alright.
    if iso.len() > ROM_SIZE as usize { return Err(ReadISOError::InvalidISO); }

    let fst_offset = read_u32(iso, HEADER_INFO_OFFSET+4);
    let entry_count = read_u32(iso, fst_offset + 0x8);
    let string_table_offset = fst_offset + entry_count * 0xC;
    let entry_start_offset = fst_offset + 0xC;

    // write regular files ---------------------------------------------------

    let mut path = PathBuf::from("./root/");
    
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

#[derive(Copy, Clone, Debug)]
pub enum IsoOp<'a> {
    Insert { iso_path: &'a Path, input_path: &'a Path },
    Delete { iso_path: &'a Path },
}

#[derive(Copy, Clone, Debug)]
enum FsEntry<'a> {
    PushDir { name: &'a str },
    PopDir,
    File {
        name: &'a str,
        offset: u32,
        size: u32,
    }
}

fn find_dir(fs: &[FsEntry], entry: &std::ffi::OsStr) -> Option<usize> {
    let mut i = 0;
    while i < fs.len() {
        match fs[i] {
            FsEntry::PushDir { name } if name == entry => return Some(i),
            FsEntry::PushDir { .. } => {
                let mut depth = 1;

                i += 1;
                // skip folder
                while i < fs.len() {
                    match &fs[i] {
                        FsEntry::File { .. } => {},
                        FsEntry::PushDir { .. } => depth += 1,
                        FsEntry::PopDir { .. } => depth -= 1,
                    }
                    i += 1;

                    if depth == 0 { break }
                }
            }
            _ => i += 1,
        }
    }

    None
}

// returns index to insert file at
fn mkdir_all<'a>(fs: &mut Vec<FsEntry<'a>>, dir_path: &'a Path) -> Result<usize, OperateISOError> {
    let mut folder_insert_idx = 0;

    let mut components = dir_path.components();
    while let Some(component) = components.next() {
        let dir_name = match component {
            std::path::Component::Normal(dir) => dir,
            std::path::Component::RootDir => continue,
            _ => return Err(OperateISOError::InvalidISOPath(dir_path.to_path_buf())),
        };

        if let Some(i) = find_dir(&fs[folder_insert_idx..], dir_name) {
            folder_insert_idx += i + 1;
        } else {
            let name = dir_name.to_str().ok_or_else(|| OperateISOError::InvalidISOPath(Path::new(dir_name).to_path_buf()))?;
            fs.insert(folder_insert_idx, FsEntry::PushDir { name });
            fs.insert(folder_insert_idx+1, FsEntry::PopDir);
            folder_insert_idx += 1;
            break;
        }
    }

    for component in components {
        let dir_name = match component {
            std::path::Component::Normal(dir) => dir,
            std::path::Component::RootDir => continue,
            _ => return Err(OperateISOError::InvalidISOPath(dir_path.to_path_buf())),
        };

        let name = dir_name.to_str().ok_or_else(|| OperateISOError::InvalidISOPath(Path::new(dir_name).to_path_buf()))?;
        fs.insert(folder_insert_idx, FsEntry::PushDir { name });
        fs.insert(folder_insert_idx+1, FsEntry::PopDir);
        folder_insert_idx += 1;
    }

    Ok(folder_insert_idx)
}

struct FilePortion<'a> {
    iso: &'a mut std::fs::File,
    size: usize,
}

impl<'a> std::io::Read for FilePortion<'a> {
    fn read(&mut self, mut buf: &mut [u8]) -> std::io::Result<usize> {
        if self.size == 0 { return Ok(0); }

        let buf_len = buf.len();
        if buf_len >= self.size {
            buf = &mut buf[..self.size];
        }

        let n = self.iso.read(buf)?;
        self.size -= n;
        Ok(n)
    }
}

pub fn read_iso_files(iso_path: &Path, files: &[(&Path, &Path)]) -> Result<(), ReadISOFilesError> {
    use std::io::{Read, Seek, SeekFrom};
    let mut iso = std::fs::File::options()
        .read(true)
        .open(iso_path)?;

    // read header ---------------------------------------------------------

    let mut buf = [0u8; 12];
    iso.seek(SeekFrom::Start(HEADER_INFO_OFFSET as _))?;
    iso.read_exact(&mut buf)?;
    let dol_offset = u32::from_be_bytes(buf[0..4].try_into().unwrap());
    let fst_offset = u32::from_be_bytes(buf[4..8].try_into().unwrap());
    let fs_size = u32::from_be_bytes(buf[8..12].try_into().unwrap());

    let mut u32_buf = [0u8; 4];
    iso.seek(SeekFrom::Start((fst_offset + 8) as _))?;
    iso.read_exact(&mut u32_buf)?;
    let entry_count = u32::from_be_bytes(u32_buf);

    let string_table_offset = fst_offset + entry_count * 0xC;
    let entry_start_offset = fst_offset + 0xC;

    // read special files ---------------------------------------------------

    for (iso_file_path, out_path) in files.iter() {
        if *iso_file_path == Path::new("ISO.hdr") {
            let mut f = std::fs::File::options()
                .create(true)
                .write(true)
                .open(out_path)?;
            iso.seek(SeekFrom::Start(0))?;
            let mut portion = FilePortion { iso: &mut iso, size: 0x2440 };
            std::io::copy(&mut portion, &mut f)?;
        }

        if *iso_file_path == Path::new("AppLoader.ldr") {
            iso.seek(SeekFrom::Start(0x2454))?;
            let mut buf = [0u8; 8];
            iso.read_exact(&mut buf)?;
            let apploader_code_size = u32::from_be_bytes(buf[0..4].try_into().unwrap());
            let apploader_trailer_size = u32::from_be_bytes(buf[4..8].try_into().unwrap());
            let size = align(apploader_code_size + apploader_trailer_size, 5) as usize;

            let mut f = std::fs::File::options()
                .create(true)
                .write(true)
                .open(out_path)?;
            iso.seek(SeekFrom::Start(0x2440))?;
            let mut portion = FilePortion { iso: &mut iso, size };
            std::io::copy(&mut portion, &mut f)?;
        }

        if *iso_file_path == Path::new("Start.dol") {
            iso.seek(SeekFrom::Start(dol_offset as _))?;
            let mut buf = vec![0u8; (fst_offset-dol_offset) as usize];
            iso.read_exact(&mut buf)?;

            let mut size = 0usize;
            for i in 0..18 {
                let segment_offset = read_u32(&buf, i*4);
                let segment_size = read_u32(&buf, 0x90 + i*4);
                let seg_end = segment_offset+segment_size;
                size = size.max(seg_end as usize);
            }

            std::fs::write(out_path, &buf[..size])?;
        }
    }

    // read iso fs ------------------------------------------------------------

    let string_table_offset_in_buf = string_table_offset - entry_start_offset; 
    iso.seek(SeekFrom::Start(entry_start_offset as _))?;
    let mut buf = vec![0u8; fs_size as usize];
    iso.read_exact(&mut buf)?;

    let mut dir_end_indices = Vec::with_capacity(8);
    let mut offset = 0;
    let mut entry_index = 1;

    let mut string_table_end = 0;

    let mut path = PathBuf::with_capacity(32);

    while offset < string_table_offset_in_buf {
        while Some(entry_index) == dir_end_indices.last().copied() {
            // dir has ended
            dir_end_indices.pop();
            path.pop();
        }

        let is_file = buf[offset as usize] == 0;

        let mut name_offset_buf = [0; 4];
        name_offset_buf[1] = buf[offset as usize+1];
        name_offset_buf[2] = buf[offset as usize+2];
        name_offset_buf[3] = buf[offset as usize+3];
        let name_offset = u32::from_be_bytes(name_offset_buf);
        let name = read_filename(&buf, string_table_offset_in_buf + name_offset)
            .ok_or(ReadISOFilesError::InvalidISO)?;

        let string_len = name.len() as u32 + 1;
        string_table_end = string_table_end.max(name_offset+string_len);

        path.push(name);
        if is_file {
            let file_offset = read_u32(&buf, offset+4);
            let file_size = read_u32(&buf, offset+8);

            for (iso_file_path, out_path) in files {
                if *iso_file_path == path.as_path() {
                    if let Some(dirs) = out_path.ancestors().nth(1) {
                        std::fs::create_dir_all(dirs)?;
                    }
                    let mut f = std::fs::File::options()
                        .create(true)
                        .write(true)
                        .open(out_path)?;
                    iso.seek(SeekFrom::Start(file_offset as _))?;
                    let mut portion = FilePortion { iso: &mut iso, size: file_size as _ };
                    std::io::copy(&mut portion, &mut f)?;
                }
            }
            path.pop();
        } else {
            let next_idx = read_u32(&buf, offset+8);
            dir_end_indices.push(next_idx);
        }

        offset += 0xC;
        entry_index += 1;
    }

    Ok(())
}

/// Tries to do as little IO as possible. 
///
/// Pass "ISO.hdr", "AppLoader.ldr", and "Start.dol" insertions to modify the ISO headers.
pub fn operate_on_iso(iso_path: &Path, ops: &[IsoOp]) -> Result<(), OperateISOError> {
    use std::io::{Read, Write, Seek, SeekFrom};

    if ops.len() == 0 { return Ok(()) }
    let iso_meta = iso_path.metadata()?;

    if iso_meta.len() > ROM_SIZE as _ { return Err(OperateISOError::InvalidISO); }

    let mut iso_file_deletions = Vec::new();
    let mut iso_file_insertions = Vec::new();

    let mut iso_hdr = None;
    let mut apploader = None;
    let mut start_dol = None;

    for op in ops {
        match op {
            IsoOp::Insert { iso_path, input_path } if *iso_path == Path::new("ISO.hdr")       => iso_hdr   = Some(input_path),
            IsoOp::Insert { iso_path, input_path } if *iso_path == Path::new("AppLoader.ldr") => apploader = Some(input_path),
            IsoOp::Insert { iso_path, input_path } if *iso_path == Path::new("Start.dol")     => start_dol = Some(input_path),

            IsoOp::Insert { iso_path, input_path } => {
                iso_file_deletions.push(*iso_path);
                iso_file_insertions.push((iso_path, input_path));
            },
            IsoOp::Delete { iso_path } => {
                iso_file_deletions.push(*iso_path);
            }
        }
    }

    let mut iso = std::fs::File::options()
        .read(true)
        .write(true)
        .open(iso_path)
        .map_err(|e| OperateISOError::OpenError { path: iso_path.into(), e })?;


    // read header ---------------------------------------------------------

    let mut buf = [0u8; 12];
    iso.seek(SeekFrom::Start(HEADER_INFO_OFFSET as _))?;
    iso.read_exact(&mut buf)?;
    let dol_offset = u32::from_be_bytes(buf[0..4].try_into().unwrap());
    let fst_offset = u32::from_be_bytes(buf[4..8].try_into().unwrap());
    let fs_size = u32::from_be_bytes(buf[8..12].try_into().unwrap());

    let mut u32_buf = [0u8; 4];
    iso.seek(SeekFrom::Start((fst_offset + 8) as _))?;
    iso.read_exact(&mut u32_buf)?;
    let entry_count = u32::from_be_bytes(u32_buf);

    let string_table_offset = fst_offset + entry_count * 0xC;
    let entry_start_offset = fst_offset + 0xC;

    // read iso fs ------------------------------------------------------------

    let string_table_offset_in_buf = string_table_offset - entry_start_offset; 
    iso.seek(SeekFrom::Start(entry_start_offset as _))?;
    let mut buf = vec![0u8; fs_size as usize];
    iso.read_exact(&mut buf)?;

    let mut dir_end_indices = Vec::with_capacity(8);
    let mut offset = 0;
    let mut entry_index = 1;

    let mut string_table_end = 0;
    let mut fs = Vec::new();

    while offset < string_table_offset_in_buf {
        while Some(entry_index) == dir_end_indices.last().copied() {
            // dir has ended
            dir_end_indices.pop();
            fs.push(FsEntry::PopDir);
        }

        let is_file = buf[offset as usize] == 0;

        let mut name_offset_buf = [0; 4];
        name_offset_buf[1] = buf[offset as usize+1];
        name_offset_buf[2] = buf[offset as usize+2];
        name_offset_buf[3] = buf[offset as usize+3];
        let name_offset = u32::from_be_bytes(name_offset_buf);
        let name = read_filename(&buf, string_table_offset_in_buf + name_offset)
            .ok_or(OperateISOError::InvalidISO)?;

        let string_len = name.len() as u32 + 1;
        string_table_end = string_table_end.max(name_offset+string_len);

        if is_file {
            let file_offset = read_u32(&buf, offset+4);
            let file_size = read_u32(&buf, offset+8);

            fs.push(FsEntry::File { name, offset: file_offset, size: file_size });
        } else {
            //let parent_idx = read_u32(&buf, offset+4); // unused
            let next_idx = read_u32(&buf, offset+8);
            dir_end_indices.push(next_idx);
            fs.push(FsEntry::PushDir { name });
        }

        offset += 0xC;
        entry_index += 1;
    }

    // operate on fs -----------------------------------------------------------

    let mut data_start = u32::MAX;
    let mut data_end = 0;

    // deletions

    let mut i = 0;
    let mut path = PathBuf::with_capacity(32);

    while i < fs.len() {
        match fs[i] {
            FsEntry::File { name, size, offset } => {
                path.push(name);

                let mut kept = true;
                let mut d = 0;
                while d < iso_file_deletions.len() {
                    if path == iso_file_deletions[d] {
                        kept = false;
                        iso_file_deletions.remove(d);
                        break;
                    }

                    d += 1;
                }

                if kept { 
                    data_start = data_start.min(offset);
                    data_end = data_end.max(size+offset);
                    i += 1; 
                } else {
                    fs.remove(i);
                }

                path.pop();
            }
            FsEntry::PushDir { name } => {
                path.push(name);
                i += 1;
            }
            FsEntry::PopDir => {
                path.pop();
                i += 1;
            }
        }
    }

    // remove empty directories

    let mut i = 0;
    while i < fs.len() {
        if matches!(fs[i], FsEntry::PushDir { .. }) && matches!(fs[i+1], FsEntry::PopDir) {
            fs.splice(i..i+2, []);
            i = i.saturating_sub(1);
        } else {
            i += 1;
        }
    }

    // find free space 

    let mut used: Vec<std::ops::Range<u32>> = Vec::with_capacity(entry_count as usize);
    for e in fs.iter() {
        match *e {
            FsEntry::File { offset, size, .. } => used.push(offset..(offset+size)),
            _ => continue,
        };
    }
    used.sort_unstable_by_key(|r| r.start);
    let mut free_space = used.windows(2)
        .filter_map(|r| {
            let a = r[0].clone();
            let b = r[1].clone();
            let new_start = align(a.end, FILE_CONTENTS_ALIGNMENT);
            if new_start >= b.start { None }
            else { Some(new_start..b.start) }
        }).collect::<Vec<_>>();

    let data_end_start = align(data_end, FILE_CONTENTS_ALIGNMENT);
    if data_end_start < ROM_SIZE { free_space.push(data_end_start..ROM_SIZE) }

    // insertions

    let mut write_locs = Vec::with_capacity(iso_file_insertions.len());

    for (iso_path, fs_path) in iso_file_insertions.iter() {
        let insert_idx = match iso_path.ancestors().nth(1) {
            Some(dir_path) => mkdir_all(&mut fs, dir_path)?,
            None => 0,
        };

        let file_name = iso_path.file_name()
            .and_then(|os_str| os_str.to_str())
            .ok_or_else(|| OperateISOError::InvalidISOPath(iso_path.to_path_buf()))?;

        let meta = fs_path.metadata()?;
        if !meta.is_file() { return Err(OperateISOError::InvalidFSPath(fs_path.to_path_buf())); }
        let size = meta.len() as u32;

        let mut offset = None;
        for free in free_space.iter_mut() {
            let free_size = free.end.saturating_sub(free.start);
            if free_size >= size {
                offset = Some(free.start);
                free.start = align(free.start+size, FILE_CONTENTS_ALIGNMENT);
                break;
            }
        }

        let offset = match offset {
            Some(o) => o,
            None => return Err(OperateISOError::ISOTooLarge),
        };

        write_locs.push(offset);
        fs.insert(insert_idx as usize, FsEntry::File { 
            name: file_name,
            size,
            offset,
        });
    }

    // new fs was created and is valid, start writing ----------------------------

    // write inserted files

    for (offset, (_, fs_path)) in write_locs.into_iter().zip(iso_file_insertions) {
        iso.seek(SeekFrom::Start(offset as _))?;

        let mut file = std::fs::File::options()
            .read(true)
            .open(fs_path)
            .map_err(|e| OperateISOError::OpenError { path: fs_path.into(), e })?;

        std::io::copy(&mut file, &mut iso)?;
    }

    // write table of contents

    let entry_count = fs.iter()
        .filter(|e| matches!(e, FsEntry::File { .. } | FsEntry::PushDir { .. }))
        .count();

    let mut toc_bytes = vec![0u8; (entry_count + 1) * 0xC];
    toc_bytes.reserve(entry_count * 16);
    let string_start = toc_bytes.len() as u32;

    toc_bytes[0] = 1;
    toc_bytes[8..12].copy_from_slice(&(entry_count as u32 + 1).to_be_bytes());

    let mut i = 1u32;
    let mut dir_start_indices = dir_end_indices;
    dir_start_indices.clear();
    dir_start_indices.push(0u32);

    for entry in fs.iter() {
        match entry {
            FsEntry::File { name, size, offset } => {
                let entry_offset = (i * 0xC) as usize;
                let string_i = toc_bytes.len() as u32 - string_start;
                toc_bytes[entry_offset+0..][..4].copy_from_slice(&string_i.to_be_bytes());
                toc_bytes[entry_offset+4..][..4].copy_from_slice(&offset.to_be_bytes());
                toc_bytes[entry_offset+8..][..4].copy_from_slice(&size.to_be_bytes());

                toc_bytes.extend_from_slice(name.as_bytes());
                toc_bytes.push(0);
                i += 1;
            },
            FsEntry::PushDir { name } => {
                let parent_idx = *dir_start_indices.last().unwrap();
                dir_start_indices.push(i);

                let entry_offset = (i * 0xC) as usize;
                let string_i = toc_bytes.len() as u32 - string_start;
                toc_bytes[entry_offset+0..][..4].copy_from_slice(&string_i.to_be_bytes());
                toc_bytes[entry_offset+0] = 1; // directory flag
                toc_bytes[entry_offset+4..][..4].copy_from_slice(&parent_idx.to_be_bytes());
                // next_idx written later

                toc_bytes.extend_from_slice(name.as_bytes());
                toc_bytes.push(0);
                i += 1;
            }
            FsEntry::PopDir => {
                let dir_idx = dir_start_indices.pop().unwrap();
                let next_idx = i;
                toc_bytes[(dir_idx*0xC+8) as usize..][..4].copy_from_slice(&next_idx.to_be_bytes());
            }
        }
    }

    if toc_bytes.len() as u32 > data_start - fst_offset {
        return Err(OperateISOError::TOCTooLarge);
    }

    iso.seek(SeekFrom::Start(fst_offset as _))?;
    iso.write_all(toc_bytes.as_slice())?;

    // write special (&&systemdata) files

    if let Some(iso_hdr) = iso_hdr {
        iso.seek(SeekFrom::Start(0))?;

        let mut f = std::fs::File::options()
            .read(true)
            .open(iso_hdr)
            .map_err(|e| OperateISOError::OpenError { path: iso_hdr.into(), e })?;
        std::io::copy(&mut f, &mut iso)?;
    }

    // overwrite necessary values in header
    let fs_size = toc_bytes.len() as u32;
    let mut buf = [0u8; 16];
    buf[ 0..][..4].copy_from_slice(&dol_offset.to_be_bytes());
    buf[ 4..][..4].copy_from_slice(&fst_offset.to_be_bytes());
    buf[ 8..][..4].copy_from_slice(&fs_size.to_be_bytes());
    buf[12..][..4].copy_from_slice(&fs_size.to_be_bytes());
    iso.seek(SeekFrom::Start(HEADER_INFO_OFFSET as _))?;
    iso.write_all(&buf)?;

    if let Some(apploader) = apploader {
        iso.seek(SeekFrom::Start(0x2440))?;

        let mut f = std::fs::File::options()
            .read(true)
            .open(apploader)
            .map_err(|e| OperateISOError::OpenError { path: apploader.into(), e })?;
        std::io::copy(&mut f, &mut iso)?;
    }

    if let Some(start_dol) = start_dol {
        iso.seek(SeekFrom::Start(dol_offset as _))?;

        let mut f = std::fs::File::options()
            .read(true)
            .open(start_dol)
            .map_err(|e| OperateISOError::OpenError { path: start_dol.into(), e })?;
        std::io::copy(&mut f, &mut iso)?;
    }

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
