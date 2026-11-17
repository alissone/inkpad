//! Just enough ZIP to read files out of an EPUB: the central directory,
//! stored and deflated entries.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub struct Entry {
    pub name: String,
    method: u16,
    comp_size: u32,
    size: u32,
    local_offset: u32,
}

pub struct Zip {
    file: File,
    pub entries: Vec<Entry>,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Entries bigger than this are refused (covers and OPF files are small).
const MAX_ENTRY: u32 = 16 << 20;

impl Zip {
    pub fn open(path: &Path) -> Result<Zip, String> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        // End of central directory: in the last 64 KiB + 22 bytes.
        let tail = len.min(65_557);
        file.seek(SeekFrom::Start(len - tail)).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; tail as usize];
        file.read_exact(&mut buf).map_err(|e| e.to_string())?;
        let eocd = (0..buf.len().saturating_sub(21)).rev().find(|&i| u32le(&buf, i) == 0x0605_4b50).ok_or("not a zip file")?;
        let count = u16le(&buf, eocd + 10) as usize;
        let cd_size = u32le(&buf, eocd + 12) as usize;
        let cd_off = u32le(&buf, eocd + 16) as u64;
        if cd_size > 8 << 20 {
            return Err("zip directory too large".into());
        }
        file.seek(SeekFrom::Start(cd_off)).map_err(|e| e.to_string())?;
        let mut cd = vec![0u8; cd_size];
        file.read_exact(&mut cd).map_err(|e| e.to_string())?;
        let mut entries = Vec::with_capacity(count);
        let mut p = 0;
        while p + 46 <= cd.len() && u32le(&cd, p) == 0x0201_4b50 {
            let (nl, xl, cl) = (u16le(&cd, p + 28) as usize, u16le(&cd, p + 30) as usize, u16le(&cd, p + 32) as usize);
            let name = String::from_utf8_lossy(cd.get(p + 46..p + 46 + nl).ok_or("bad zip entry")?).into_owned();
            entries.push(Entry { name, method: u16le(&cd, p + 10), comp_size: u32le(&cd, p + 20), size: u32le(&cd, p + 24), local_offset: u32le(&cd, p + 42) });
            p += 46 + nl + xl + cl;
        }
        Ok(Zip { file, entries })
    }

    /// Read an entry by exact name (case-insensitive fallback).
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>, String> {
        let i = self
            .entries
            .iter()
            .position(|e| e.name == name)
            .or_else(|| self.entries.iter().position(|e| e.name.eq_ignore_ascii_case(name)))
            .ok_or_else(|| format!("{name} not in zip"))?;
        let (method, comp, size, off) = {
            let e = &self.entries[i];
            (e.method, e.comp_size, e.size, e.local_offset)
        };
        if comp > MAX_ENTRY || size > MAX_ENTRY {
            return Err(format!("{name} too large"));
        }
        let mut hdr = [0u8; 30];
        self.file.seek(SeekFrom::Start(off as u64)).map_err(|e| e.to_string())?;
        self.file.read_exact(&mut hdr).map_err(|e| e.to_string())?;
        if u32le(&hdr, 0) != 0x0403_4b50 {
            return Err("bad local header".into());
        }
        let skip = u16le(&hdr, 26) as i64 + u16le(&hdr, 28) as i64;
        self.file.seek(SeekFrom::Current(skip)).map_err(|e| e.to_string())?;
        let mut data = vec![0u8; comp as usize];
        self.file.read_exact(&mut data).map_err(|e| e.to_string())?;
        match method {
            0 => Ok(data),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(&data, MAX_ENTRY as usize).map_err(|e| format!("inflate: {e:?}")),
            m => Err(format!("unsupported compression {m}")),
        }
    }
}
