use std::io;
use digest::Digest;
use hex_view::HexView;


pub(crate) fn hasher<H: Digest, R: io::Read>(mut reader: R) -> io::Result<String> {
    let mut buffer = [0u8; 8 * 1024];
    let mut hasher = H::new();

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 { break }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", HexView::from(hasher.finalize().as_slice())))
}
