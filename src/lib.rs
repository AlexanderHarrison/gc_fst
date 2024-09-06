const OFFSET_TO_DOL_OFFSET: u32 = 0x420;
const OFFSET_TO_FST_OFFSET: u32 = 0x424;

#[derive(Debug)]
pub enum ReadISOError {
    InvalidISO,
    WriteFileError(std::io::Error),
    CreateDirError(std::io::Error),
}

#[derive(Debug)]
pub enum WriteISOError {
    ISOTooLarge,
    ReadFileError(std::io::Error),
    ReadDirError(std::io::Error),
}

const ROM_SIZE: u32 = 0x57058000;

pub fn write_iso(root: &std::path::Path) -> Result<Vec<u8>, WriteISOError> {
    const SEGMENT_ALIGNMENT: u32 = 8;

    let mut iso = Vec::with_capacity(ROM_SIZE as usize);
    let mut path = root.to_path_buf();
    
    // write special files -------------------------------------------------

    path.push("&&systemdata");


    path.push("ISO.hdr");
    let iso_offset = iso.len();
    //println!("write ISO.hdr {}", iso_offset);
    let iso_size;
    {
        let mut header_file = std::fs::File::open(&path).map_err(|e| WriteISOError::ReadFileError(e))?;
        iso_size = std::io::copy(&mut header_file, &mut iso).map_err(|e| WriteISOError::ReadFileError(e))?;
    }
    path.pop();
    // TODO overwritten later: fst_offset, dol_offset, fst_size, max_fst_size,


    path.push("AppLoader.ldr");
    let apploader_offset = iso.len();
    //println!("write AppLoader.ldr {}", apploader_offset);
    let apploader_size;
    {
        let mut apploader_file = std::fs::File::open(&path).map_err(|e| WriteISOError::ReadFileError(e))?;
        apploader_size = std::io::copy(&mut apploader_file, &mut iso).map_err(|e| WriteISOError::ReadFileError(e))?;
    }
    path.pop();
    // overwritten later: n/a


    let rounded_size = align(iso.len() as u32, SEGMENT_ALIGNMENT);
    iso.resize(rounded_size as usize, 0u8);


    path.push("Start.dol");
    let dol_offset = iso.len();
    //println!("write Start.dol {}", dol_offset);
    let dol_size;
    {
        let mut dol_file = std::fs::File::open(&path).map_err(|e| WriteISOError::ReadFileError(e))?;
        dol_size = std::io::copy(&mut dol_file, &mut iso).map_err(|e| WriteISOError::ReadFileError(e))?;
    }
    path.pop();
    // overwritten later: n/a


    let rounded_size = align(iso.len() as u32, SEGMENT_ALIGNMENT);
    iso.resize(rounded_size as usize, 0u8);

    // pop &&systemdata
    path.pop();

    // write filesystem header, string table, and contents ---------------------------------------

    let fst_offset = iso.len() as u32;

    let (entry_count, total_string_length) = count_entries(&path)?;
    //println!("entry count {}", entry_count + 1);
    iso.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
    // entry count also includes this header
    iso.extend_from_slice(&(entry_count+1).to_be_bytes());

    let fs_start = iso.len() as u32;
    let fs_end = fs_start + 0xC * entry_count;
    let string_start = fs_end;
    let string_end = string_start + total_string_length;
    iso.resize(string_end as usize, 0u8);

    let entry_start = fs_start;
    let mut entry_offset = entry_start;
    let mut string_offset = string_start;

    //println!("write fst {}", fst_offset);
    //println!("write strings {}", string_start);

    let rounded_size = align(iso.len() as u32, 15);
    iso.resize(rounded_size as usize, 0u8);
    //println!("write contents {}", iso.len());

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
    
    //let order = match path.file_name().and_then(|s| s.to_str()) {
    //    Some("root") => ROOT_ORDER,
    //    Some("audio") => AUDIO_ORDER,
    //    Some("us") => US_ORDER,
    //    _ => panic!(),
    //};

    // temp testing work
    //iso.resize(ROM_SIZE as usize, 0u8);
    //for (a, file_offset) in order {
    //    let entry = path.join(a);

    for entry in std::fs::read_dir(&path).map_err(|e| WriteISOError::ReadDirError(e))? {
        let entry = entry.map_err(|e| WriteISOError::ReadDirError(e))?;
        let metadata = entry.metadata().map_err(|e| WriteISOError::ReadDirError(e))?;
        if metadata.is_file() {
            let rounded_size = align(iso.len() as u32, FILE_CONTENTS_ALIGNMENT);
            iso.resize(rounded_size as usize, 0u8);

            // entry data
            iso[*entry_offset as usize..][..4].copy_from_slice(&(*string_offset - string_start).to_be_bytes());
            let contents_offset = iso.len() as u32;
            //let contents_offset = *file_offset as u32;
            iso[*entry_offset as usize+4..][..4].copy_from_slice(&contents_offset.to_be_bytes());
            iso[*entry_offset as usize+8..][..4].copy_from_slice(&(metadata.len() as u32).to_be_bytes());
            *entry_offset += 12;

            // file name
            //let file_name = entry.file_name().unwrap();
            let file_name = entry.file_name();
            let file_name_len = file_name.len() as u32;
            iso[*string_offset as usize..][..file_name_len as usize].copy_from_slice(file_name.as_encoded_bytes());
            iso[(*string_offset + file_name_len) as usize] = 0; // null terminator
            *string_offset += file_name_len + 1;

            // contents
            path.push(&file_name);
            let mut file = std::fs::File::open(&path).map_err(|e| WriteISOError::ReadFileError(e))?;
            std::io::copy(&mut file, iso).map_err(|e| WriteISOError::ReadFileError(e))?;
            //std::io::copy(&mut file, &mut std::io::Cursor::new(&mut iso[*file_offset..][..metadata.len() as usize])).map_err(|e| WriteISOError::ReadFileError(e))?;
            path.pop();
        } else if metadata.is_dir() {
            //let file_name = entry.file_name().unwrap();
            let file_name = entry.file_name();
            if file_name == "&&systemdata" { continue; }

            // entry data
            let string_offset_from_start = *string_offset - string_start;
            let mut w0 = string_offset_from_start.to_be_bytes();
            w0[0] = 1; // directory flag
            iso[*entry_offset as usize..][..4].copy_from_slice(&w0);
            iso[*entry_offset as usize+4..][..4].copy_from_slice(&parent_dir_idx.to_be_bytes());
            // next idx written later
            let next_idx_offset = *entry_offset + 8;
            *entry_offset += 12;

            // file name
            let file_name_len = file_name.len() as u32;
            iso[*string_offset as usize..][..file_name_len as usize].copy_from_slice(file_name.as_encoded_bytes());
            iso[(*string_offset + file_name_len) as usize] = 0; // null terminator
            *string_offset += file_name_len + 1;

            let sub_dir = path.join(&file_name);
            let entry_index = (*entry_offset - entry_start) / 12; // 1-based index, so compute after 12 byte increment was added.
            write_dir(
                &sub_dir,
                iso,
                entry_index,
                entry_start,
                entry_offset,
                string_start,
                string_offset,
            )?;

            let next_idx = (*entry_offset - entry_start) / 12 + 1;
            iso[next_idx_offset as usize..][..4].copy_from_slice(&next_idx.to_be_bytes());
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
                let new_path = path.join(&file_name); // must realloc due to borrowing issues
                let (ec, sl) = count_entries(&new_path)?;
                entry_count += ec;
                total_string_length += sl;
            }

            // skip symlinks
            _ => (),
        }
    }

    Ok((entry_count, total_string_length))
}

pub fn read_iso(iso: &[u8]) -> Result<(), ReadISOError> {
    let fst_offset = read_u32(iso, OFFSET_TO_FST_OFFSET);
    let entry_count = read_u32(iso, fst_offset + 0x8);
    let string_table_offset = fst_offset + entry_count * 0xC;
    let entry_start_offset = fst_offset + 0xC;

    // write regular files ---------------------------------------------------

    let mut path = std::path::PathBuf::from("./root/");
    std::fs::create_dir_all(&path).map_err(|e| ReadISOError::CreateDirError(e))?;

    let mut dir_end_indices = Vec::with_capacity(8);
    let mut offset = entry_start_offset;
    let mut entry_index = 1;
    let mut start = std::u32::MAX;
    while offset < string_table_offset {
        while Some(entry_index) == dir_end_indices.last().copied() {
            // dir has ended
            dir_end_indices.pop();
            println!("END DIR {}", path.display());
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
            println!("    (\"{}\", {}),", &filename, file_offset);
            let file_size = read_u32(iso, offset+8);

            start = start.min(file_offset);

            path.push(filename);

            std::fs::write(&path, &iso[file_offset as usize..][..file_size as usize])
            //std::fs::write(&path, &iso[file_offset as usize..][..16 as usize])
                .map_err(|e| ReadISOError::WriteFileError(e))?;
            path.pop();
        } else {
            let parent_idx = read_u32(iso, offset+4); // unused
            let next_idx = read_u32(iso, offset+8);
            dir_end_indices.push(next_idx);
            path.push(filename);
            println!("DIR {}", path.display());
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
    //println!("read ISO.hdr {}", 0);
    std::fs::write(&path, &iso[0..0x2440])
        .map_err(|e| ReadISOError::WriteFileError(e))?;
    path.pop();

    path.push("AppLoader.ldr");
    //println!("read AppLoader.ldr {}", 0x2440);
    let apploader_code_size = read_u32(iso, 0x2454);
    let apploader_trailer_size = read_u32(iso, 0x2458);
    let apploader_total_size = align(apploader_code_size + apploader_trailer_size, 5);
    let apploader_end = 0x2440 + apploader_total_size;
    std::fs::write(&path, &iso[0x2440..apploader_end as usize])
        .map_err(|e| ReadISOError::WriteFileError(e))?;
    path.pop();

    path.push("Start.dol");
    let dol_offset = read_u32(iso, OFFSET_TO_DOL_OFFSET);
    //println!("read Start.dol {}", dol_offset);
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

fn write_u32(iso: &mut [u8], offset: u32, n: u32) {
    iso[offset as usize..][..4].copy_from_slice(&n.to_be_bytes());
}

fn read_u32(iso: &[u8], offset: u32) -> u32 {
    u32::from_be_bytes(iso[offset as usize..][..4].try_into().unwrap())
}

fn read_filename(iso: &[u8], offset: u32) -> Option<&str> {
    std::ffi::CStr::from_bytes_until_nul(&iso[offset as usize..]).ok()?.to_str().ok()
}

fn align(n: u32, bits: u32) -> u32 { 
    let mask = (1 << bits) - 1;
    (n + mask) & !mask
}

const AUDIO_ORDER: &'static [(&'static str, usize)] = &[
    ("1padv.ssm", 1194910176),
    ("1pend.ssm", 1195071360),
    ("1p_qk.hps", 1008965088),
    ("akaneia.hps", 1009969376),
    ("baloon.hps", 1014488128),
    ("bigblue.hps", 1016626336),
    ("bigblue.ssm", 1195435520),
    ("captain.ssm", 1195470720),
    ("castle.hps", 1020800608),
    ("castle.ssm", 1195916160),
    ("clink.ssm", 1196092896),
    ("continue.hps", 1024091392),
    ("corneria.hps", 1024347200),
    ("corneria.ssm", 1196394208),
    ("dk.ssm", 1196837792),
    ("docmari.hps", 1026916544),
    ("drmario.ssm", 1197045856),
    ("emblem.ssm", 1197479168),
    ("end.ssm", 1197978016),
    ("ending.hps", 914182268),
    ("ending.ssm", 1197979648),
    ("falco.ssm", 1199150432),
    ("famidemo.hps", 1029805280),
    ("ff_1p01.hps", 1030736576),
    ("ff_1p02.hps", 1030941248),
    ("ff_bad.hps", 1031164224),
    ("ff_dk.hps", 1031267200),
    ("ff_emb.hps", 1031530464),
    ("ff_flat.hps", 1031912608),
    ("ff_fox.hps", 1032141088),
    ("ff_fzero.hps", 1032380064),
    ("ff_good.hps", 1032604128),
    ("ff_ice.hps", 1032727840),
    ("ff_kirby.hps", 1033007872),
    ("ff_link.hps", 1033165280),
    ("ff_mario.hps", 1033425184),
    ("ff_nes.hps", 1033720960),
    ("ff_poke.hps", 1034066368),
    ("ff_samus.hps", 1034353376),
    ("ff_step1.hps", 1034569696),
    ("ff_step2.hps", 1034703872),
    ("ff_step3.hps", 1034846752),
    ("ff_yoshi.hps", 1035004032),
    ("flatzone.hps", 1035378560),
    ("fourside.hps", 1039349216),
    ("fourside.ssm", 1199754400),
    ("fox.ssm", 1199833152),
    ("gameover.hps", 1043675424),
    ("ganon.ssm", 1200410464),
    ("garden.hps", 1043827328),
    ("garden.ssm", 1200840448),
    ("gkoopa.ssm", 1201269856),
    ("greatbay.hps", 1051833504),
    ("greatbay.ssm", 1201760320),
    ("greens.hps", 1053569472),
    ("greens.ssm", 1202251520),
    ("gw.ssm", 1202396448),
    ("howto.hps", 649282940),
    ("howto_s.hps", 1057301920),
    ("hyaku.hps", 1061114176),
    ("hyaku2.hps", 1064832160),
    ("ice.ssm", 1202563392),
    ("icemt.hps", 1068186272),
    ("inis1_01.hps", 1072219744),
    ("inis1_02.hps", 1075506880),
    ("inis2_01.hps", 1076941056),
    ("inis2_02.hps", 1078802720),
    ("intro_es.hps", 1079219264),
    ("intro_nm.hps", 1079344384),
    ("item_h.hps", 1079614240),
    ("item_s.hps", 1079810624),
    ("izumi.hps", 1079994464),
    ("kirby.ssm", 1203044256),
    ("kirbytm.ssm", 1203634304),
    ("klaid.ssm", 1204210944),
    ("kongo.hps", 1085939328),
    ("kongo.ssm", 1204515968),
    ("koopa.ssm", 1204876256),
    ("kraid.hps", 1092993536),
    ("last.ssm", 1205405792),
    ("link.ssm", 1206069376),
    ("luigi.ssm", 1206400800),
    ("main.ssm", 1206774496),
    ("mario.ssm", 1208839648),
    ("mars.ssm", 1209214720),
    ("menu01.hps", 1098433440),
    ("menu02.hps", 1100519264),
    ("menu3.hps", 1102540224),
    ("mewtwo.ssm", 1209723872),
    ("mhands.ssm", 1210290880),
    ("mrider.hps", 1104763200),
    ("mutecity.hps", 1109558144),
    ("mutecity.ssm", 1210856256),
    ("ness.ssm", 1211007232),
    ("nr_1p.ssm", 1211518880),
    ("nr_name.ssm", 1211641600),
    ("nr_select.ssm", 1211993664),
    ("nr_title.ssm", 1212232896),
    ("nr_vs.ssm", 1212386304),
    ("old_dk.hps", 1113436928),
    ("old_kb.hps", 1119187200),
    ("old_ys.hps", 1121669440),
    ("onett.ssm", 1212441024),
    ("onetto.hps", 1123834112),
    ("onetto2.hps", 1128050656),
    ("opening.hps", 779283356),
    ("peach.ssm", 1212495776),
    ("pichu.ssm", 1212914752),
    ("pikachu.ssm", 1213497408),
    ("pokemon.ssm", 1214113312),
    ("pokesta.hps", 1134852832),
    ("pstadium.hps", 1138434240),
    ("pstadium.ssm", 1214678400),
    ("pupupu.ssm", 1214780192),
    ("pura.hps", 1140737728),
    ("purin.ssm", 1214903680),
    ("rcruise.hps", 1146252448),
    ("samus.ssm", 1215207488),
    ("saria.hps", 1149560640),
    ("shrine.hps", 1150885152),
    ("siren.hps", 1154681056),
    ("smari3.hps", 1157421024),
    ("smash2.sem", 1008700540),
    ("sp_end.hps", 1160929824),
    ("sp_giga.hps", 1164200160),
    ("sp_metal.hps", 1167379072),
    ("sp_zako.hps", 1169448640),
    ("swm_15min.hps", 514833212),
    ("s_info1.hps", 1172595328),
    ("s_info2.hps", 1172717888),
    ("s_info3.hps", 1172835840),
    ("s_new1.hps", 1172940096),
    ("s_new2.hps", 1173016192),
    ("s_newcom.hps", 1173187616),
    ("s_select.hps", 1173381376),
    ("target.hps", 1173577760),
    ("us", 0),
    ("venom.hps", 1175120736),
    ("venom.ssm", 1237241088),
    ("vl_battle.hps", 1177424288),
    ("vl_castle.hps", 1177745024),
    ("vl_corneria.hps", 1178042592),
    ("vl_cosmos.hps", 1178507808),
    ("vl_figure1.hps", 1178802112),
    ("vl_figure2.hps", 1179098464),
    ("vl_fzero.hps", 1179272512),
    ("vl_last_v2.hps", 1179446624),
    ("vs_hyou1.hps", 1179786464),
    ("vs_hyou2.hps", 1180914272),
    ("yorster.hps", 1182041376),
    ("yoshi.ssm", 1237625216),
    ("ystory.hps", 1184024640),
    ("zebes.hps", 1188371872),
    ("zebes.ssm", 1237941568),
    ("zs.ssm", 1238330528),
];

const US_ORDER: &'static [(&'static str, usize)] = &[
    ("1padv.ssm", 1215528480),
    ("1pend.ssm", 1215689664),
    ("bigblue.ssm", 1216053824),
    ("captain.ssm", 1216089024),
    ("castle.ssm", 1216522368),
    ("clink.ssm", 1216699104),
    ("corneria.ssm", 1217000416),
    ("dk.ssm", 1217400416),
    ("drmario.ssm", 1217608480),
    ("emblem.ssm", 1218024960),
    ("end.ssm", 1218509888),
    ("ending.ssm", 1218511520),
    ("falco.ssm", 1219682304),
    ("fourside.ssm", 1220193696),
    ("fox.ssm", 1220272448),
    ("ganon.ssm", 1220750688),
    ("garden.ssm", 1221170432),
    ("gkoopa.ssm", 1221599840),
    ("greatbay.ssm", 1222090304),
    ("greens.ssm", 1222581504),
    ("gw.ssm", 1222726432),
    ("ice.ssm", 1222887840),
    ("kirby.ssm", 1223353728),
    ("kirbytm.ssm", 1223934432),
    ("klaid.ssm", 1224511072),
    ("kongo.ssm", 1224816096),
    ("koopa.ssm", 1225176384),
    ("last.ssm", 1225694560),
    ("link.ssm", 1226358144),
    ("luigi.ssm", 1226678144),
    ("main.ssm", 1227053696),
    ("mario.ssm", 1229118848),
    ("mars.ssm", 1229493920),
    ("mewtwo.ssm", 1230010336),
    ("mhands.ssm", 1230465248),
    ("mutecity.ssm", 1231030624),
    ("ness.ssm", 1231181600),
    ("nr_1p.ssm", 1231693248),
    ("nr_name.ssm", 1231815968),
    ("nr_select.ssm", 1232155296),
    ("nr_title.ssm", 1232396256),
    ("nr_vs.ssm", 1232549664),
    ("onett.ssm", 1232604384),
    ("peach.ssm", 1232659136),
    ("pichu.ssm", 1233063968),
    ("pikachu.ssm", 1233625376),
    ("pokemon.ssm", 1234232448),
    ("pstadium.ssm", 1234730240),
    ("pupupu.ssm", 1234832032),
    ("purin.ssm", 1234955520),
    ("samus.ssm", 1235291744),
    ("smash2.sem", 1008832904),
    ("venom.ssm", 1235603136),
    ("yoshi.ssm", 1235943712),
    ("zebes.ssm", 1236267584),
    ("zs.ssm", 1236656544),
];

const ROOT_ORDER: &'static [(&'static str, usize)] = &[
    ("audio", 0),
    ("DbCo.dat", 1252753408),
    ("EfCaData.dat", 1330642944),
    ("EfCoData.dat", 1427767296),
    ("EfDkData.dat", 1340669952),
    ("EfFeData.dat", 1422098432),
    ("EfFxData.dat", 1355350016),
    ("EfGnData.dat", 1417871360),
    ("EfIcData.dat", 1326022656),
    ("EfKbCa.dat", 1334149120),
    ("EfKbData.dat", 1358954496),
    ("EfKbDk.dat", 1344012288),
    ("EfKbFe.dat", 1425571840),
    ("EfKbFx.dat", 1357053952),
    ("EfKbGn.dat", 1420001280),
    ("EfKbIc.dat", 1329070080),
    ("EfKbKp.dat", 1364951040),
    ("EfKbLg.dat", 1373503488),
    ("EfKbMr.dat", 1377632256),
    ("EfKbMs.dat", 1382449152),
    ("EfKbPc.dat", 1400242176),
    ("EfKbPk.dat", 1402634240),
    ("EfKbSs.dat", 1410433024),
    ("EfKbZd.dat", 1322123264),
    ("EfKpData.dat", 1362755584),
    ("EfLgData.dat", 1371668480),
    ("EfLkData.dat", 1367015424),
    ("EfMnData.dat", 1427734528),
    ("EfMrData.dat", 1375076352),
    ("EfMsData.dat", 1379205120),
    ("EfMtData.dat", 1384677376),
    ("EfNsData.dat", 1388871680),
    ("EfPeData.dat", 1392672768),
    ("EfPkData.dat", 1401683968),
    ("EfPrData.dat", 1404108800),
    ("EfSsData.dat", 1408204800),
    ("EfYsData.dat", 1412333568),
    ("EfZdData.dat", 1318060032),
    ("GmEvent.dat", 1442611200),
    ("GmGoAnim.dat", 1252786176),
    ("GmGoCoin.dat", 1252818944),
    ("GmGover.dat", 1252851712),
    ("GmIntEz.dat", 1458241536),
    ("GmKumite.dat", 1252950016),
    ("GmPause.dat", 1459781632),
    ("GmPause.usd", 1459847040),
    ("GmRegClr.dat", 1459159040),
    ("GmRegClr.usd", 1458963284),
    ("GmRegEnd.dat", 1252982784),
    ("GmRegEnd.usd", 1240927520),
    ("GmRegendAdventureCaptain.thp", 19451348),
    ("GmRegendAdventureClink.thp", 19543296),
    ("GmRegendAdventureDonkey.thp", 19640816),
    ("GmRegendAdventureDrmario.thp", 19735188),
    ("GmRegendAdventureFalco.thp", 19828052),
    ("GmRegendAdventureFox.thp", 19909120),
    ("GmRegendAdventureGamewatch.thp", 20004308),
    ("GmRegendAdventureGanon.thp", 20089008),
    ("GmRegendAdventureKirby.thp", 20185096),
    ("GmRegendAdventureKoopa.thp", 20269704),
    ("GmRegendAdventureLink.thp", 20359320),
    ("GmRegendAdventureLuigi.thp", 20453644),
    ("GmRegendAdventureMario.thp", 20552096),
    ("GmRegendAdventureMarth.thp", 20641204),
    ("GmRegendAdventureMewtwo.thp", 20735804),
    ("GmRegendAdventureNess.thp", 20831916),
    ("GmRegendAdventurePeach.thp", 20919096),
    ("GmRegendAdventurePichu.thp", 21007016),
    ("GmRegendAdventurePikachu.thp", 21096520),
    ("GmRegendAdventurePoponana.thp", 21171780),
    ("GmRegendAdventurePurin.thp", 21246740),
    ("GmRegendAdventureRoy.thp", 21343428),
    ("GmRegendAdventureSamus.thp", 21433932),
    ("GmRegendAdventureYoshi.thp", 21525044),
    ("GmRegendAdventureZeldaseak.thp", 21611220),
    ("GmRegendAllstarCaptain.thp", 21709440),
    ("GmRegendAllstarClink.thp", 21799312),
    ("GmRegendAllstarDonkey.thp", 21895296),
    ("GmRegendAllstarDrmario.thp", 21978888),
    ("GmRegendAllstarFalco.thp", 22076144),
    ("GmRegendAllstarFox.thp", 22151732),
    ("GmRegendAllstarGamewatch.thp", 22244676),
    ("GmRegendAllstarGanon.thp", 22342860),
    ("GmRegendAllstarKirby.thp", 22433516),
    ("GmRegendAllstarKoopa.thp", 22523352),
    ("GmRegendAllstarLink.thp", 22613516),
    ("GmRegendAllstarLuigi.thp", 22708808),
    ("GmRegendAllstarMario.thp", 22798888),
    ("GmRegendAllstarMarth.thp", 22895544),
    ("GmRegendAllstarMewtwo.thp", 22974996),
    ("GmRegendAllstarNess.thp", 23070836),
    ("GmRegendAllstarPeach.thp", 23167068),
    ("GmRegendAllstarPichu.thp", 23236172),
    ("GmRegendAllstarPikachu.thp", 23318240),
    ("GmRegendAllstarPoponana.thp", 23399108),
    ("GmRegendAllstarPurin.thp", 23485500),
    ("GmRegendAllstarRoy.thp", 23575832),
    ("GmRegendAllstarSamus.thp", 23670456),
    ("GmRegendAllstarYoshi.thp", 23762292),
    ("GmRegendAllstarZeldaseak.thp", 23855632),
    ("GmRegendSimpleCaptain.thp", 23940448),
    ("GmRegendSimpleClink.thp", 24039804),
    ("GmRegendSimpleDonkey.thp", 24136364),
    ("GmRegendSimpleDrmario.thp", 24231012),
    ("GmRegendSimpleFalco.thp", 24326784),
    ("GmRegendSimpleFox.thp", 24407588),
    ("GmRegendSimpleGamewatch.thp", 24504568),
    ("GmRegendSimpleGanon.thp", 24598148),
    ("GmRegendSimpleKirby.thp", 24685544),
    ("GmRegendSimpleKoopa.thp", 24770104),
    ("GmRegendSimpleLink.thp", 24862548),
    ("GmRegendSimpleLuigi.thp", 24951064),
    ("GmRegendSimpleMario.thp", 25040968),
    ("GmRegendSimpleMarth.thp", 25135940),
    ("GmRegendSimpleMewtwo.thp", 25232316),
    ("GmRegendSimpleNess.thp", 25304120),
    ("GmRegendSimplePeach.thp", 25385420),
    ("GmRegendSimplePichu.thp", 25479976),
    ("GmRegendSimplePikachu.thp", 25571288),
    ("GmRegendSimplePoponana.thp", 25665096),
    ("GmRegendSimplePurin.thp", 25759480),
    ("GmRegendSimpleRoy.thp", 25850048),
    ("GmRegendSimpleSamus.thp", 25942224),
    ("GmRegendSimpleYoshi.thp", 26030228),
    ("GmRegendSimpleZeldaseak.thp", 26125148),
    ("GmRgEBG2.dat", 1253310464),
    ("GmRgEBG3.dat", 1253507072),
    ("GmRgStnd.dat", 1253703680),
    ("GmRst.dat", 1443168256),
    ("GmRst.usd", 1442677652),
    ("GmRstMCa.dat", 1443758080),
    ("GmRstMCl.dat", 1443856384),
    ("GmRstMDk.dat", 1443954688),
    ("GmRstMDr.dat", 1444052992),
    ("GmRstMFc.dat", 1444151296),
    ("GmRstMFe.dat", 1444282368),
    ("GmRstMFx.dat", 1444380672),
    ("GmRstMGn.dat", 1444446208),
    ("GmRstMGw.dat", 1444544512),
    ("GmRstMKb.dat", 1444610048),
    ("GmRstMKp.dat", 1444773888),
    ("GmRstMLg.dat", 1444872192),
    ("GmRstMLk.dat", 1444970496),
    ("GmRstMMr.dat", 1445068800),
    ("GmRstMMs.dat", 1445134336),
    ("GmRstMMt.dat", 1445232640),
    ("GmRstMNs.dat", 1445298176),
    ("GmRstMPc.dat", 1445396480),
    ("GmRstMPe.dat", 1445494784),
    ("GmRstMPk.dat", 1445625856),
    ("GmRstMPn.dat", 1445691392),
    ("GmRstMPr.dat", 1445888000),
    ("GmRstMSk.dat", 1446051840),
    ("GmRstMSs.dat", 1446117376),
    ("GmRstMYs.dat", 1446215680),
    ("GmRstMZd.dat", 1446313984),
    ("GmStRoll.dat", 1253736448),
    ("GmTitle.dat", 1255997440),
    ("GmTitle.usd", 1241240072),
    ("GmTou1p.dat", 1256325120),
    ("GmTou1p.usd", 1241490800),
    ("GmTou2p.dat", 1256456192),
    ("GmTou2p.usd", 1241616996),
    ("GmTou3p.dat", 1256620032),
    ("GmTou3p.usd", 1241736820),
    ("GmTou4p.dat", 1257111552),
    ("GmTou4p.usd", 1242137636),
    ("GmTrain.dat", 1257373696),
    ("GmTrain.usd", 1242378936),
    ("GmTtAll.dat", 1437171712),
    ("GmTtAll.usd", 1436895452),
    ("GrBb.dat", 1257472000),
    ("GrCn.dat", 1259765760),
    ("GrCn.usd", 1242460816),
    ("GrCs.dat", 1261502464),
    ("GrEF1.dat", 1263042560),
    ("GrEF2.dat", 1263468544),
    ("GrEF3.dat", 1263894528),
    ("GrFs.dat", 1264517120),
    ("GrFz.dat", 1265434624),
    ("GrGb.dat", 1265958912),
    ("GrGd.dat", 1266843648),
    ("GrGr.dat", 1267466240),
    ("GrHe.dat", 1268350976),
    ("GrHr.dat", 1269202944),
    ("GrHr.usd", 1244110444),
    ("GrI1.dat", 1269989376),
    ("GrI2.dat", 1270251520),
    ("GrIm.dat", 1270448128),
    ("GrIz.dat", 1272020992),
    ("GrKg.dat", 1273167872),
    ("GrKr.dat", 1273659392),
    ("GrMc.dat", 1274806272),
    ("GrNBa.dat", 1276182528),
    ("GrNBr.dat", 1276641280),
    ("GrNFg.dat", 1278083072),
    ("GrNKr.dat", 1278541824),
    ("GrNLa.dat", 1279918080),
    ("GrNPo.dat", 1280540672),
    ("GrNSr.dat", 1281196032),
    ("GrNZr.dat", 1282867200),
    ("GrOk.dat", 1283915776),
    ("GrOp.dat", 1285062656),
    ("GrOt.dat", 1285750784),
    ("GrOt.usd", 1244890280),
    ("GrOy.dat", 1286897664),
    ("GrPs.dat", 1287159808),
    ("GrPs.usd", 1246018600),
    ("GrPs1.dat", 1288634368),
    ("GrPs2.dat", 1288962048),
    ("GrPs3.dat", 1289191424),
    ("GrPs4.dat", 1289519104),
    ("GrPu.dat", 1289814016),
    ("GrRc.dat", 1290829824),
    ("GrSh.dat", 1291812864),
    ("GrSt.dat", 1293058048),
    ("GrTCa.dat", 1293910016),
    ("GrTCl.dat", 1294499840),
    ("GrTDk.dat", 1294630912),
    ("GrTDr.dat", 1294893056),
    ("GrTe.dat", 1295122432),
    ("GrTFc.dat", 1295810560),
    ("GrTFe.dat", 1296105472),
    ("GrTFx.dat", 1296203776),
    ("GrTGn.dat", 1296859136),
    ("GrTGw.dat", 1297022976),
    ("GrTIc.dat", 1297285120),
    ("GrTKb.dat", 1297514496),
    ("GrTKp.dat", 1297743872),
    ("GrTLg.dat", 1298104320),
    ("GrTLk.dat", 1298464768),
    ("GrTMr.dat", 1298989056),
    ("GrTMs.dat", 1299382272),
    ("GrTMt.dat", 1299546112),
    ("GrTNs.dat", 1299677184),
    ("GrTPc.dat", 1299939328),
    ("GrTPe.dat", 1300168704),
    ("GrTPk.dat", 1300725760),
    ("GrTPr.dat", 1300856832),
    ("GrTSk.dat", 1301053440),
    ("GrTSs.dat", 1301118976),
    ("GrTYs.dat", 1301708800),
    ("GrTZd.dat", 1301872640),
    ("GrVe.dat", 1302036480),
    ("GrVe.usd", 1247479624),
    ("GrYt.dat", 1304363008),
    ("GrZe.dat", 1304952832),
    ("IfAll.dat", 1432092672),
    ("IfAll.usd", 1435993888),
    ("IfCoGet.dat", 1459912704),
    ("IfComSn.dat", 1305837568),
    ("IfComSn.usd", 1249714308),
    ("IfHrNoCn.dat", 1306198016),
    ("IfHrNoCn.usd", 1250047448),
    ("IfHrReco.dat", 1306230784),
    ("IfHrReco.usd", 1250056400),
    ("IfPrize.dat", 1306296320),
    ("IfPrize.usd", 1250093148),
    ("IfVsCam.dat", 1306361856),
    ("IfVsCam.usd", 1250132108),
    ("IrAls.dat", 1458274304),
    ("IrAls.usd", 1458645292),
    ("IrEzFigG.dat", 1306460160),
    ("IrEzFigG.usd", 1250208836),
    ("IrEzTarg.dat", 1306820608),
    ("IrEzTarg.usd", 1250551560),
    ("IrEzTuki.dat", 1307115520),
    ("IrEzTuki.usd", 1250821356),
    ("IrNml.dat", 1457455104),
    ("IrNml.usd", 1456755492),
    ("IrRdMap.dat", 1458601984),
    ("IrRdMap.usd", 1458941948),
    ("ItCo.dat", 1429110784),
    ("ItCo.usd", 1433020468),
    ("LbAd.dat", 1442643968),
    ("LbBf.dat", 1459945472),
    ("LbMcGame.dat", 1439891456),
    ("LbMcGame.usd", 1437570948),
    ("LbMcSnap.dat", 1440022528),
    ("LbMcSnap.usd", 1437598144),
    ("LbRb.dat", 1439858688),
    ("LbRf.dat", 1459552256),
    ("MnExtAll.dat", 1455357952),
    ("MnExtAll.usd", 1450197872),
    ("MnMaAll.dat", 1440055296),
    ("MnMaAll.usd", 1437605948),
    ("MnNamedef.dat", 1307443200),
    ("MnSlChr.dat", 1451524096),
    ("MnSlChr.usd", 1446386468),
    ("MnSlMap.dat", 1307475968),
    ("MnSlMap.usd", 1251137572),
    ("MvEndCaptain.mth", 782722556),
    ("MvEndClink.mth", 787506812),
    ("MvEndDonkey.mth", 792223132),
    ("MvEndDrmario.mth", 796882780),
    ("MvEndFalco.mth", 802214524),
    ("MvEndFox.mth", 806683068),
    ("MvEndGamewatch.mth", 811132188),
    ("MvEndGanon.mth", 818183580),
    ("MvEndKirby.mth", 822106428),
    ("MvEndKoopa.mth", 827582364),
    ("MvEndLink.mth", 832216284),
    ("MvEndLuigi.mth", 837795836),
    ("MvEndMario.mth", 843434556),
    ("MvEndMarth.mth", 848274812),
    ("MvEndMewtwo.mth", 853559260),
    ("MvEndNess.mth", 858081468),
    ("MvEndPeach.mth", 863346396),
    ("MvEndPichu.mth", 868939228),
    ("MvEndPikachu.mth", 875424508),
    ("MvEndPoponana.mth", 880431996),
    ("MvEndPurin.mth", 886317372),
    ("MvEndRoy.mth", 891898172),
    ("MvEndSamus.mth", 897735164),
    ("MvEndYoshi.mth", 901759356),
    ("MvEndZelda.mth", 908793980),
    ("MvHowto.mth", 545751356),
    ("MvOmake15.mth", 26210556),
    ("MvOpen.mth", 652246652),
    ("NtAppro.dat", 1308164096),
    ("NtAppro.usd", 1251774384),
    ("NtMemAc.dat", 1439924224),
    ("NtMemAc.usd", 1437591108),
    ("NtMsgWin.dat", 1439956992),
    ("NtProge.dat", 1308327936),
    ("opening.bnr", 19444852),
    ("PdPm.dat", 1459585024),
    ("PlBo.dat", 1313505280),
    ("PlBoAJ.dat", 1313701888),
    ("PlBoNr.dat", 1313603584),
    ("PlCa.dat", 1330479104),
    ("PlCaAJ.dat", 1334312960),
    ("PlCaBu.dat", 1331363840),
    ("PlCaDViWaitAJ.dat", 1240072192),
    ("PlCaGr.dat", 1331920896),
    ("PlCaGy.dat", 1332477952),
    ("PlCaNr.dat", 1330806784),
    ("PlCaRe.dat", 1333035008),
    ("PlCaRe.usd", 1251924764),
    ("PlCaWh.dat", 1333592064),
    ("PlCh.dat", 1312784384),
    ("PlChAJ.dat", 1313177600),
    ("PlChNr.dat", 1312849920),
    ("PlCl.dat", 1335951360),
    ("PlClAJ.dat", 1338605568),
    ("PlClBk.dat", 1336705024),
    ("PlClBu.dat", 1337163776),
    ("PlClDViWaitAJ.dat", 1240104960),
    ("PlClNr.dat", 1336246272),
    ("PlClRe.dat", 1337622528),
    ("PlClWh.dat", 1338081280),
    ("PlCo.dat", 1459617792),
    ("PlDk.dat", 1340473344),
    ("PlDkAJ.dat", 1345323008),
    ("PlDkBk.dat", 1341390848),
    ("PlDkBu.dat", 1342046208),
    ("PlDkDViWaitAJ.dat", 1240137728),
    ("PlDkGr.dat", 1342701568),
    ("PlDkNr.dat", 1340735488),
    ("PlDkRe.dat", 1343356928),
    ("PlDr.dat", 1346928640),
    ("PlDrAJ.dat", 1349681152),
    ("PlDrBk.dat", 1347682304),
    ("PlDrBu.dat", 1348173824),
    ("PlDrDViWaitAJ.dat", 1240170496),
    ("PlDrGr.dat", 1348665344),
    ("PlDrNr.dat", 1347190784),
    ("PlDrRe.dat", 1349156864),
    ("PlFc.dat", 1350991872),
    ("PlFcAJ.dat", 1353580544),
    ("PlFcBu.dat", 1351516160),
    ("PlFcDViWaitAJ.dat", 1240203264),
    ("PlFcGr.dat", 1351778304),
    ("PlFcNr.dat", 1351254016),
    ("PlFcRe.dat", 1352040448),
    ("PlFe.dat", 1421803520),
    ("PlFeAJ.dat", 1425702912),
    ("PlFeBu.dat", 1422819328),
    ("PlFeDViWaitAJ.dat", 1240236032),
    ("PlFeGr.dat", 1423507456),
    ("PlFeNr.dat", 1422131200),
    ("PlFeRe.dat", 1424195584),
    ("PlFeYe.dat", 1424883712),
    ("PlFx.dat", 1355087872),
    ("PlFxAJ.dat", 1357185024),
    ("PlFxDViWaitAJ.dat", 1240268800),
    ("PlFxGr.dat", 1355874304),
    ("PlFxLa.dat", 1356267520),
    ("PlFxNr.dat", 1355481088),
    ("PlFxOr.dat", 1356660736),
    ("PlGk.dat", 1309736960),
    ("PlGkAJ.dat", 1310490624),
    ("PlGkNr.dat", 1309966336),
    ("PlGl.dat", 1315176448),
    ("PlGlAJ.dat", 1315405824),
    ("PlGlNr.dat", 1315307520),
    ("PlGn.dat", 1417641984),
    ("PlGnAJ.dat", 1420132352),
    ("PlGnBu.dat", 1418428416),
    ("PlGnDViWaitAJ.dat", 1240301568),
    ("PlGnGr.dat", 1418821632),
    ("PlGnLa.dat", 1419214848),
    ("PlGnNr.dat", 1418035200),
    ("PlGnRe.dat", 1419608064),
    ("PlGw.dat", 1415938048),
    ("PlGwAJ.dat", 1416495104),
    ("PlGwDViWaitAJ.dat", 1240334336),
    ("PlGwNr.dat", 1416200192),
    ("PlKb.dat", 1358725120),
    ("PlKbAJ.dat", 1360822272),
    ("PlKbBu.dat", 1359347712),
    ("PlKbBuCpDk.dat", 1344339968),
    ("PlKbBuCpFc.dat", 1352597504),
    ("PlKbBuCpMt.dat", 1386151936),
    ("PlKbBuCpPr.dat", 1405779968),
    ("PlKbCpCa.dat", 1334247424),
    ("PlKbCpCl.dat", 1338540032),
    ("PlKbCpDk.dat", 1344077824),
    ("PlKbCpDr.dat", 1349648384),
    ("PlKbCpFc.dat", 1352302592),
    ("PlKbCpFe.dat", 1425604608),
    ("PlKbCpFx.dat", 1357086720),
    ("PlKbCpGn.dat", 1420099584),
    ("PlKbCpGw.dat", 1416331264),
    ("PlKbCpKp.dat", 1365016576),
    ("PlKbCpLg.dat", 1373437952),
    ("PlKbCpLk.dat", 1369702400),
    ("PlKbCpMr.dat", 1377665024),
    ("PlKbCpMs.dat", 1382350848),
    ("PlKbCpMt.dat", 1385824256),
    ("PlKbCpNs.dat", 1390575616),
    ("PlKbCpPc.dat", 1400274944),
    ("PlKbCpPe.dat", 1396768768),
    ("PlKbCpPk.dat", 1402503168),
    ("PlKbCpPp.dat", 1329135616),
    ("PlKbCpPr.dat", 1405485056),
    ("PlKbCpSk.dat", 1322221568),
    ("PlKbCpSs.dat", 1410498560),
    ("PlKbCpYs.dat", 1414299648),
    ("PlKbCpZd.dat", 1322156032),
    ("PlKbDViWaitAJ.dat", 1240367104),
    ("PlKbGr.dat", 1359642624),
    ("PlKbGrCpDk.dat", 1344536576),
    ("PlKbGrCpFc.dat", 1352794112),
    ("PlKbGrCpMt.dat", 1386348544),
    ("PlKbGrCpPr.dat", 1405976576),
    ("PlKbNr.dat", 1359052800),
    ("PlKbNrCpDk.dat", 1344143360),
    ("PlKbNrCpFc.dat", 1352400896),
    ("PlKbNrCpGw.dat", 1416364032),
    ("PlKbNrCpMt.dat", 1385955328),
    ("PlKbNrCpPr.dat", 1405583360),
    ("PlKbRe.dat", 1359937536),
    ("PlKbReCpDk.dat", 1344733184),
    ("PlKbReCpFc.dat", 1352990720),
    ("PlKbReCpMt.dat", 1386545152),
    ("PlKbReCpPr.dat", 1406173184),
    ("PlKbWh.dat", 1360232448),
    ("PlKbWhCpDk.dat", 1344929792),
    ("PlKbWhCpFc.dat", 1353187328),
    ("PlKbWhCpMt.dat", 1386741760),
    ("PlKbWhCpPr.dat", 1406369792),
    ("PlKbYe.dat", 1360527360),
    ("PlKbYeCpDk.dat", 1345126400),
    ("PlKbYeCpFc.dat", 1353383936),
    ("PlKbYeCpMt.dat", 1386938368),
    ("PlKbYeCpPr.dat", 1406566400),
    ("PlKp.dat", 1362558976),
    ("PlKpAJ.dat", 1365082112),
    ("PlKpBk.dat", 1363378176),
    ("PlKpBu.dat", 1363902464),
    ("PlKpDViWaitAJ.dat", 1240399872),
    ("PlKpNr.dat", 1362853888),
    ("PlKpRe.dat", 1364426752),
    ("PlLg.dat", 1371471872),
    ("PlLgAJ.dat", 1373536256),
    ("PlLgAq.dat", 1372160000),
    ("PlLgDViWaitAJ.dat", 1240432640),
    ("PlLgNr.dat", 1371766784),
    ("PlLgPi.dat", 1372553216),
    ("PlLgWh.dat", 1372946432),
    ("PlLk.dat", 1366753280),
    ("PlLkAJ.dat", 1369767936),
    ("PlLkBk.dat", 1367605248),
    ("PlLkBu.dat", 1368129536),
    ("PlLkDViWaitAJ.dat", 1240465408),
    ("PlLkNr.dat", 1367080960),
    ("PlLkRe.dat", 1368653824),
    ("PlLkWh.dat", 1369178112),
    ("PlMh.dat", 1312096256),
    ("PlMhAJ.dat", 1312489472),
    ("PlMhNr.dat", 1312161792),
    ("PlMr.dat", 1374879744),
    ("PlMrAJ.dat", 1377730560),
    ("PlMrBk.dat", 1375666176),
    ("PlMrBu.dat", 1376157696),
    ("PlMrDViWaitAJ.dat", 1240498176),
    ("PlMrGr.dat", 1376649216),
    ("PlMrNr.dat", 1375174656),
    ("PlMrYe.dat", 1377140736),
    ("PlMs.dat", 1379008512),
    ("PlMsAJ.dat", 1382481920),
    ("PlMsBk.dat", 1379860480),
    ("PlMsDViWaitAJ.dat", 1240530944),
    ("PlMsGr.dat", 1380483072),
    ("PlMsNr.dat", 1379237888),
    ("PlMsRe.dat", 1381105664),
    ("PlMsWh.dat", 1381728256),
    ("PlMt.dat", 1384448000),
    ("PlMtAJ.dat", 1387134976),
    ("PlMtBu.dat", 1385037824),
    ("PlMtDViWaitAJ.dat", 1240563712),
    ("PlMtGr.dat", 1385299968),
    ("PlMtNr.dat", 1384742912),
    ("PlMtRe.dat", 1385562112),
    ("PlNn.dat", 1326088192),
    ("PlNnAJ.dat", 1330413568),
    ("PlNnAq.dat", 1327988736),
    ("PlNnNr.dat", 1326546944),
    ("PlNnWh.dat", 1328709632),
    ("PlNnYe.dat", 1327267840),
    ("PlNs.dat", 1388511232),
    ("PlNsAJ.dat", 1390673920),
    ("PlNsBu.dat", 1389395968),
    ("PlNsDViWaitAJ.dat", 1240596480),
    ("PlNsGr.dat", 1389789184),
    ("PlNsNr.dat", 1389002752),
    ("PlNsYe.dat", 1390182400),
    ("PlPc.dat", 1398964224),
    ("PlPcAJ.dat", 1400406016),
    ("PlPcBu.dat", 1399422976),
    ("PlPcDViWaitAJ.dat", 1240629248),
    ("PlPcGr.dat", 1399685120),
    ("PlPcNr.dat", 1399226368),
    ("PlPcRe.dat", 1400012800),
    ("PlPe.dat", 1392377856),
    ("PlPeAJ.dat", 1396834304),
    ("PlPeBu.dat", 1393524736),
    ("PlPeDViWaitAJ.dat", 1240662016),
    ("PlPeGr.dat", 1394343936),
    ("PlPeNr.dat", 1392705536),
    ("PlPeWh.dat", 1395163136),
    ("PlPeYe.dat", 1395982336),
    ("PlPk.dat", 1401454592),
    ("PlPkAJ.dat", 1402667008),
    ("PlPkBu.dat", 1401978880),
    ("PlPkDViWaitAJ.dat", 1240694784),
    ("PlPkGr.dat", 1402142720),
    ("PlPkNr.dat", 1401815040),
    ("PlPkRe.dat", 1402306560),
    ("PlPp.dat", 1325891584),
    ("PlPpAJ.dat", 1329233920),
    ("PlPpDViWaitAJ.dat", 1240727552),
    ("PlPpGr.dat", 1326907392),
    ("PlPpNr.dat", 1326186496),
    ("PlPpOr.dat", 1327628288),
    ("PlPpRe.dat", 1328349184),
    ("PlPr.dat", 1403977728),
    ("PlPrAJ.dat", 1406763008),
    ("PlPrBu.dat", 1404403712),
    ("PlPrDViWaitAJ.dat", 1240760320),
    ("PlPrGr.dat", 1404665856),
    ("PlPrNr.dat", 1404141568),
    ("PlPrRe.dat", 1404928000),
    ("PlPrYe.dat", 1405190144),
    ("PlSb.dat", 1317601280),
    ("PlSbAJ.dat", 1317699584),
    ("PlSbNr.dat", 1317634048),
    ("PlSk.dat", 1318158336),
    ("PlSkAJ.dat", 1324515328),
    ("PlSkBu.dat", 1319469056),
    ("PlSkDViWaitAJ.dat", 1240793088),
    ("PlSkGr.dat", 1320222720),
    ("PlSkNr.dat", 1318715392),
    ("PlSkRe.dat", 1320976384),
    ("PlSkWh.dat", 1321730048),
    ("PlSs.dat", 1407877120),
    ("PlSsAJ.dat", 1410564096),
    ("PlSsBk.dat", 1408729088),
    ("PlSsDViWaitAJ.dat", 1240825856),
    ("PlSsGr.dat", 1409155072),
    ("PlSsLa.dat", 1409581056),
    ("PlSsNr.dat", 1408303104),
    ("PlSsPi.dat", 1410007040),
    ("PlYs.dat", 1412038656),
    ("PlYsAJ.dat", 1414430720),
    ("PlYsAq.dat", 1412661248),
    ("PlYsBu.dat", 1412988928),
    ("PlYsDViWaitAJ.dat", 1240858624),
    ("PlYsNr.dat", 1412366336),
    ("PlYsPi.dat", 1413316608),
    ("PlYsRe.dat", 1413644288),
    ("PlYsYe.dat", 1413971968),
    ("PlZd.dat", 1317863424),
    ("PlZdAJ.dat", 1322319872),
    ("PlZdBu.dat", 1319108608),
    ("PlZdDViWaitAJ.dat", 1240891392),
    ("PlZdGr.dat", 1319862272),
    ("PlZdNr.dat", 1318354944),
    ("PlZdRe.dat", 1320615936),
    ("PlZdWh.dat", 1321369600),
    ("SdClr.dat", 1459355648),
    ("SdClr.usd", 1459148972),
    ("SdDec.dat", 1459388416),
    ("SdDec.usd", 1459149176),
    ("SdIntro.dat", 1459879936),
    ("SdMenu.dat", 1442217984),
    ("SdMenu.usd", 1439761260),
    ("SdMsgBox.dat", 1439989760),
    ("SdMsgBox.usd", 1437595268),
    ("SdPrize.dat", 1308360704),
    ("SdPrize.usd", 1252483304),
    ("SdProge.dat", 1308459008),
    ("SdProge.usd", 1252500580),
    ("SdRst.dat", 1443659776),
    ("SdRst.usd", 1443153972),
    ("SdSlChr.dat", 1456668672),
    ("SdSlChr.usd", 1451513196),
    ("SdStRoll.dat", 1308491776),
    ("SdTou.dat", 1308721152),
    ("SdTou.usd", 1252501368),
    ("SdToy.dat", 1442545664),
    ("SdToy.usd", 1439848820),
    ("SdToyExp.dat", 1308819456),
    ("SdToyExp.usd", 1252516528),
    ("SdTrain.dat", 1309474816),
    ("SdTrain.usd", 1252738180),
    ("SdVsCam.dat", 1309507584),
    ("SdVsCam.usd", 1252751628),
    ("SmSt.dat", 1309540352),
    ("TmBox.dat", 1309704192),
    ("TyAligat.dat", 914948096),
    ("Tyandold.dat", 914980864),
    ("TyAndruf.dat", 915013632),
    ("TyAnnie.dat", 915472384),
    ("TyArwing.dat", 915734528),
    ("TyAyumi.dat", 915898368),
    ("TyBacket.dat", 916160512),
    ("TyBalf.dat", 916193280),
    ("TyBancho.dat", 916389888),
    ("TyBarCan.dat", 916488192),
    ("TyBaritm.dat", 916520960),
    ("TyBayone.dat", 916586496),
    ("TyBField.dat", 916750336),
    ("TyBKoopa.dat", 916914176),
    ("TyBMario.dat", 917340160),
    ("TyBox.dat", 917667840),
    ("TyBrdian.dat", 917700608),
    ("TyBSword.dat", 917798912),
    ("TyBTrper.dat", 917831680),
    ("TyCaptan.dat", 917864448),
    ("TyCaptnR.dat", 918618112),
    ("TyCaptR2.dat", 919175168),
    ("TyCathar.dat", 919732224),
    ("TyCerebi.dat", 919764992),
    ("TyChico.dat", 920092672),
    ("TyClink.dat", 920158208),
    ("TyClinkR.dat", 920911872),
    ("TyClnkR2.dat", 921239552),
    ("TyCoin.dat", 921534464),
    ("TyCpeacch.dat", 921600000),
    ("TyCpR2Us.dat", 922189824),
    ("TyCrobat.dat", 922746880),
    ("TyCulCul.dat", 922877952),
    ("TyCupsul.dat", 923074560),
    ("TyDaikon.dat", 923172864),
    ("TyDaisy.dat", 923205632),
    ("TyDataf.dat", 924188672),
    ("TyDatai.dat", 1459486720),
    ("TyDatai.usd", 1459532908),
    ("TyDedede.dat", 924221440),
    ("TyDiskun.dat", 924516352),
    ("TyDixKng.dat", 924581888),
    ("TyDkJr.dat", 924909568),
    ("TyDLight.dat", 925696000),
    ("TyDMario.dat", 925892608),
    ("TyDnkyR2.dat", 926351360),
    ("TyDonkey.dat", 926842880),
    ("TyDonkyR.dat", 927498240),
    ("TyDosei.dat", 927989760),
    ("TyDosin.dat", 928022528),
    ("TyDossun.dat", 928350208),
    ("TyDrMriR.dat", 928382976),
    ("TyDrMrR2.dat", 928841728),
    ("TyDuck.dat", 929136640),
    ("TyEgg.dat", 929398784),
    ("TyEievui.dat", 929431552),
    ("TyEntei.dat", 929890304),
    ("TyEtcA.dat", 930217984),
    ("TyEtcB.dat", 930414592),
    ("TyEtcC.dat", 930545664),
    ("TyEtcD.dat", 930643968),
    ("TyEtcE.dat", 930742272),
    ("TyExbike.dat", 930938880),
    ("TyFalco.dat", 931233792),
    ("TyFalcoR.dat", 931299328),
    ("TyFalcR2.dat", 931561472),
    ("TyFFlowr.dat", 931758080),
    ("TyFFlyer.dat", 931790848),
    ("TyFire.dat", 932478976),
    ("TyFirest.dat", 932544512),
    ("TyFliper.dat", 933724160),
    ("TyFood.dat", 933756928),
    ("TyFounta.dat", 933953536),
    ("TyFox.dat", 934871040),
    ("TyFoxR.dat", 935002112),
    ("TyFoxR2.dat", 935493632),
    ("TyFreeze.dat", 935821312),
    ("TyFrezer.dat", 935854080),
    ("TyFubana.dat", 935919616),
    ("TyFudane.dat", 936345600),
    ("TyFzero.dat", 936378368),
    ("TyFZone.dat", 936738816),
    ("TyGanond.dat", 936837120),
    ("TyGanonR.dat", 937689088),
    ("TyGanonR2.dat", 938016768),
    ("TyGKoopa.dat", 938409984),
    ("TyGldFox.dat", 938835968),
    ("TyGmCube.dat", 939491328),
    ("TyGooie.dat", 940015616),
    ("TyGoron.dat", 940212224),
    ("TyGrtfox.dat", 940605440),
    ("TyGShell.dat", 941228032),
    ("TyGWatch.dat", 941293568),
    ("TyGWathR.dat", 941359104),
    ("TyGWatR2.dat", 941424640),
    ("TyGwfeld.dat", 941621248),
    ("TyHagane.dat", 941850624),
    ("TyHammer.dat", 942080000),
    ("TyHarise.dat", 942112768),
    ("TyHassam.dat", 942145536),
    ("TyHDosin.dat", 942637056),
    ("TyHeart.dat", 942768128),
    ("TyHecros.dat", 942800896),
    ("TyHeiho.dat", 943292416),
    ("TyHeriri.dat", 943357952),
    ("TyHinoar.dat", 943489024),
    ("TyHitode.dat", 943882240),
    ("TyHomBat.dat", 943915008),
    ("TyHotRly.dat", 943947776),
    ("TyHouou.dat", 944472064),
    ("TyIceclm.dat", 944537600),
    ("TyIceclR.dat", 944570368),
    ("TyIcecR2.dat", 945061888),
    ("TyItemA.dat", 945520640),
    ("TyItemB.dat", 945684480),
    ("TyItemC.dat", 945717248),
    ("TyItemD.dat", 945782784),
    ("TyItemE.dat", 946012160),
    ("TyJeff.dat", 946176000),
    ("TyJugemu.dat", 946470912),
    ("TyKabigo.dat", 946634752),
    ("TyKamex.dat", 946700288),
    ("TyKamiwa.dat", 947191808),
    ("TyKart.dat", 947257344),
    ("TyKasumi.dat", 947519488),
    ("TyKbBall.dat", 948043776),
    ("TyKbFigt.dat", 948174848),
    ("TyKbFire.dat", 948502528),
    ("TyKbHat1.dat", 948895744),
    ("TyKbHat2.dat", 950435840),
    ("TyKbHat3.dat", 951812096),
    ("TyKbHat4.dat", 953221120),
    ("TyKbHat5.dat", 954138624),
    ("TyKiller.dat", 955056128),
    ("TyKingCr.dat", 955187200),
    ("TyKinopi.dat", 956399616),
    ("TyKirbR2.dat", 956727296),
    ("TyKirby.dat", 956858368),
    ("TyKirbyR.dat", 957054976),
    ("TyKirei.dat", 957153280),
    ("TyKoopa.dat", 957349888),
    ("TyKoopaR.dat", 957743104),
    ("TyKopaR2.dat", 958136320),
    ("TyKpMobl.dat", 958365696),
    ("TyKraid.dat", 958660608),
    ("TyKuribo.dat", 958955520),
    ("TyKusuda.dat", 959217664),
    ("TyLandms.dat", 959250432),
    ("TyLeaded.dat", 960299008),
    ("TyLight.dat", 960331776),
    ("TyLikeli.dat", 960397312),
    ("TyLink.dat", 960430080),
    ("TyLinkR.dat", 961216512),
    ("TyLinkR2.dat", 961576960),
    ("TyLipSti.dat", 961904640),
    ("TyLizdon.dat", 961937408),
    ("TyLucky.dat", 962035712),
    ("TyLugia.dat", 962068480),
    ("TyLuigi.dat", 962428928),
    ("TyLuigiM.dat", 963870720),
    ("TyLuigiR.dat", 964001792),
    ("TyLuigR2.dat", 964526080),
    ("TyMajora.dat", 964788224),
    ("TyMapA.dat", 965476352),
    ("TyMapB.dat", 965672960),
    ("TyMapC.dat", 965771264),
    ("TyMapD.dat", 965869568),
    ("TyMapE.dat", 965967872),
    ("TyMaril.dat", 966033408),
    ("TyMarin.dat", 966098944),
    ("TyMario.dat", 966557696),
    ("TyMarioR.dat", 966885376),
    ("TyMariR2.dat", 967180288),
    ("TyMars.dat", 967442432),
    ("TyMarsR.dat", 967901184),
    ("TyMarsR2.dat", 968556544),
    ("TyMarumi.dat", 968982528),
    ("TyMatado.dat", 969048064),
    ("TyMbombJ.dat", 969179136),
    ("TyMBombU.dat", 969211904),
    ("TyMCapsu.dat", 969244672),
    ("TyMcCmDs.dat", 969342976),
    ("TyMcR1Ds.dat", 969900032),
    ("TyMcR2Ds.dat", 970293248),
    ("TyMetamo.dat", 970719232),
    ("TyMetoid.dat", 970784768),
    ("TyMew.dat", 971505664),
    ("TyMew2.dat", 971571200),
    ("TyMew2R.dat", 972128256),
    ("TyMew2R2.dat", 972455936),
    ("TyMHandL.dat", 972619776),
    ("TyMhandR.dat", 972914688),
    ("TyMHige.dat", 973209600),
    ("TyMKnigt.dat", 973438976),
    ("TyMMario.dat", 973733888),
    ("TyMnBg.dat", 973799424),
    ("TyMnDisp.dat", 974749696),
    ("TyMnDisp.usd", 914438524),
    ("TyMnFigp.dat", 974979072),
    ("TyMnFigp.usd", 914624780),
    ("TyMnInfo.dat", 975208448),
    ("TyMnInfo.usd", 914847376),
    ("TyMnView.dat", 975241216),
    ("TyMnView.usd", 914867796),
    ("TyMoon.dat", 975339520),
    ("TyMrCoin.dat", 975372288),
    ("TyMRider.dat", 975405056),
    ("TyMrMant.dat", 976224256),
    ("TyMrTail.dat", 976846848),
    ("TyMsBall.dat", 977043456),
    ("TyMSword.dat", 977076224),
    ("TyMtlbox.dat", 977108992),
    ("TyMTomat.dat", 977141760),
    ("TyMucity.dat", 977174528),
    ("TyMuroom.dat", 977862656),
    ("TyMycCmA.dat", 977895424),
    ("TyMycCmB.dat", 978059264),
    ("TyMycCmC.dat", 978092032),
    ("TyMycCmD.dat", 978223104),
    ("TyMycCmE.dat", 978288640),
    ("TyMycR1A.dat", 978321408),
    ("TyMycR1B.dat", 978518016),
    ("TyMycR1C.dat", 978550784),
    ("TyMycR1D.dat", 978649088),
    ("TyMycR1E.dat", 978714624),
    ("TyMycR2A.dat", 978747392),
    ("TyMycR2B.dat", 978944000),
    ("TyMycR2C.dat", 979009536),
    ("TyMycR2D.dat", 979075072),
    ("TyMycR2E.dat", 979140608),
    ("TyNasubi.dat", 979238912),
    ("TyNess.dat", 979369984),
    ("TyNessR.dat", 979632128),
    ("TyNessR2.dat", 979959808),
    ("TyNoko.dat", 980287488),
    ("TyNyathR.dat", 980549632),
    ("TyNyoroz.dat", 980779008),
    ("TyOcarin.dat", 980811776),
    ("TyOctaro.dat", 980942848),
    ("TyOni.dat", 981008384),
    ("TyOokido.dat", 981270528),
    ("TyOrima.dat", 981630976),
    ("TyOtosei.dat", 981762048),
    ("TyParaso.dat", 981794816),
    ("TyPatapa.dat", 981827584),
    ("TyPchuR2.dat", 981860352),
    ("TyPeach.dat", 982024192),
    ("TyPeachR.dat", 983040000),
    ("TyPeacR2.dat", 983728128),
    ("TyPeppy.dat", 984121344),
    ("TyPichu.dat", 984219648),
    ("TyPichuR.dat", 984350720),
    ("TyPikacR.dat", 984449024),
    ("TyPikacu.dat", 984547328),
    ("TyPikaR2.dat", 984678400),
    ("TyPikmin.dat", 984809472),
    ("TyPippi.dat", 985137152),
    ("TyPit.dat", 985169920),
    ("TyPlum.dat", 985825280),
    ("TyPMario.dat", 986120192),
    ("TyPMurom.dat", 986251264),
    ("TyPokeA.dat", 986284032),
    ("TyPokeB.dat", 986546176),
    ("TyPokeC.dat", 986578944),
    ("TyPokeD.dat", 986644480),
    ("TyPokeE.dat", 986873856),
    ("TyPola.dat", 986906624),
    ("TyPoo.dat", 987463680),
    ("TyPorgn2.dat", 987758592),
    ("TyPupuri.dat", 987955200),
    ("TyPurin.dat", 988020736),
    ("TyPurinR.dat", 988119040),
    ("TyPuriR2.dat", 988217344),
    ("TyPy.dat", 988348416),
    ("TyQChan.dat", 988381184),
    ("TyQuesD.dat", 988512256),
    ("TyRaikou.dat", 988545024),
    ("TyRaygun.dat", 988774400),
    ("TyRayMk2.dat", 988807168),
    ("TyReset.dat", 989102080),
    ("TyRick.dat", 989134848),
    ("TyRidley.dat", 989429760),
    ("TyRodori.dat", 990248960),
    ("TyRoMilk.dat", 990281728),
    ("TyRoy.dat", 990347264),
    ("TyRoyR.dat", 990773248),
    ("TyRoyR2.dat", 991264768),
    ("TyRShell.dat", 991756288),
    ("TySamuR2.dat", 991821824),
    ("TySamus.dat", 991887360),
    ("TySamusM.dat", 992411648),
    ("TySamusR.dat", 993067008),
    ("TyScBall.dat", 993361920),
    ("TySeak.dat", 993394688),
    ("TySeakR.dat", 994738176),
    ("TySeakR2.dat", 995295232),
    ("TySeirei.dat", 995557376),
    ("TySeriA.dat", 995655680),
    ("TySeriB.dat", 995885056),
    ("TySeriC.dat", 995983360),
    ("TySeriD.dat", 996081664),
    ("TySeriE.dat", 996212736),
    ("TySherif.dat", 996409344),
    ("TySlippy.dat", 996442112),
    ("TySmShip.dat", 996540416),
    ("TySndbag.dat", 997097472),
    ("TySnZero.dat", 997261312),
    ("TySonans.dat", 998146048),
    ("TySpyclJ.dat", 998277120),
    ("TySpyclU.dat", 998309888),
    ("TySScope.dat", 998342656),
    ("TyStand.dat", 998375424),
    ("TyStandD.dat", 998473728),
    ("TyStar.dat", 998506496),
    ("TyStarod.dat", 998539264),
    ("TyStdiam.dat", 998572032),
    ("TyStnley.dat", 999096320),
    ("TyStrman.dat", 999358464),
    ("TySuikun.dat", 999522304),
    ("TyTamagn.dat", 999915520),
    ("TyTanuki.dat", 1000144896),
    ("TyTarget.dat", 1000177664),
    ("TyTenEit.dat", 1000210432),
    ("TyTeresa.dat", 1000636416),
    ("TyThnder.dat", 1000833024),
    ("TyTogepy.dat", 1000865792),
    ("TyToppi.dat", 1000964096),
    ("TyTosaki.dat", 1001029632),
    ("TyTosanz.dat", 1001095168),
    ("TyTotake.dat", 1001127936),
    ("TyTwinkl.dat", 1001160704),
    ("TyUfo.dat", 1001193472),
    ("TyUKnown.dat", 1001226240),
    ("TyUsahat.dat", 1001455616),
    ("TyUsokie.dat", 1001521152),
    ("TyVirus.dat", 1001652224),
    ("TyWalugi.dat", 1001914368),
    ("TyWanino.dat", 1002536960),
    ("TyWario.dat", 1002668032),
    ("TyWaveRc.dat", 1003061248),
    ("TyWdldee.dat", 1004142592),
    ("TyWolfen.dat", 1004339200),
    ("TyWpStar.dat", 1004503040),
    ("TyWtBear.dat", 1004535808),
    ("TyWtCat.dat", 1004601344),
    ("TyWvRcUs.dat", 1004896256),
    ("TyWwoods.dat", 1005977600),
    ("TyYoshi.dat", 1006174208),
    ("TyYoshiR.dat", 1006600192),
    ("TyYoshR2.dat", 1006895104),
    ("TyZelda.dat", 1007190016),
    ("TyZeldaR.dat", 1007616000),
    ("TyZeldR2.dat", 1008074752),
    ("TyZeniga.dat", 1008336896),
    ("TyZkMen.dat", 1008435200),
    ("TyZkPair.dat", 1008500736),
    ("TyZkWmen.dat", 1008631808),
    ("usa.ini", 4587520),
    ("Vi0102.dat", 1238925312),
    ("Vi0401.dat", 1238990848),
    ("Vi0402.dat", 1239056384),
    ("Vi0501.dat", 1239187456),
    ("Vi0502.dat", 1239285760),
    ("Vi0601.dat", 1239318528),
    ("Vi0801.dat", 1239547904),
    ("Vi1101.dat", 1239580672),
    ("Vi1201v1.dat", 1239646208),
    ("Vi1201v2.dat", 1239678976),
    ("Vi1202.dat", 1240039424),
];

