use std::io::{self, BufRead, BufReader};
use std::fs::File;

use sha2::{Sha256, Digest};

pub fn sha2_256_digest(file: File) -> io::Result<String> {
    let mut hasher = Sha256::default();
    let data = &mut BufReader::new(file);
    loop {
        let read = {
            let buffer = data.fill_buf()?;
            if buffer.len() == 0 { break }
            hasher.input(buffer);
            buffer.len()
        };

        data.consume(read);
    }

    Ok(format!("{:x}", hasher.result()))
}
