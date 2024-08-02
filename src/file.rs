use std::fs::{ File, OpenOptions };
use std::io::{Seek, SeekFrom, Read, Write};

pub(crate) fn touch_and_fill_with_zeros(file_path: &str, desired_length: usize) -> Result<(), anyhow::Error> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(file_path)?;
    let data: Vec<u8> = vec![0; desired_length];
    file.write_all(&data)?;
    Ok(())
}

pub(crate) fn read_piece_from(file_path: &str, begin: usize, length: usize) -> Result<Vec<u8>, anyhow::Error> {
    let mut file = File::open(file_path)?;
    file.seek(SeekFrom::Start(begin as u64))?;
    let mut buffer: Vec<u8> = vec![0; length];
    file.read_exact(&mut buffer)?;
    Ok(buffer)
}

pub(crate) fn write_piece_to(file_path: &str, begin: usize, piece: &[u8]) -> Result<(), anyhow::Error> {
    let mut file = OpenOptions::new()
        .write(true)
        .open(file_path)?;
    file.seek(SeekFrom::Start(begin as u64))?;
    file.write_all(piece)?;
    Ok(())
}

//TODO: Add tests using utilities for creating temporary files
