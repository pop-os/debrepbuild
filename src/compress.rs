use bus_writer::BusWriter;
use deflate::Compression;
use deflate::write::GzEncoder;
use std::fs::File;
use std::io;
use std::path::Path;
use xz2::write::XzEncoder;

pub const UNCOMPRESSED: u8 = 0b1;
pub const GZ_COMPRESS: u8 = 0b10;
pub const XZ_COMPRESS: u8 = 0b100;
pub const ZSTD_COMPRESS: u8 = 0b1000;

pub trait SyncWrite: Send + Sync + io::Write {}
impl<T: Send + Sync + io::Write> SyncWrite for T {}

pub fn compress<R: io::Read>(name: &str, path: &Path, stream: R, support: u8) -> io::Result<()> {
    inner_compress(name, path, stream, support).map_err(|why| {
        io::Error::new(
            io::ErrorKind::Other,
            format!(
                "failed to compress output to {} in {}: {}",
                name,
                path.display(),
                why
            ),
        )
    })
}

fn inner_compress<R: io::Read>(name: &str, path: &Path, stream: R, support: u8) -> io::Result<()> {
    if support == 0 {
        return Ok(());
    }

    let mut destinations = {
        let mut writers: Vec<Box<dyn SyncWrite>> = Vec::new();
        if support & UNCOMPRESSED != 0 {
            writers.push(Box::new(File::create(path.join(name))?));
        }

        if support & GZ_COMPRESS != 0 {
            let gz_file = File::create(path.join([name, ".gz"].concat()))?;
            writers.push(Box::new(GzEncoder::new(gz_file, Compression::Best)));
        }

        if support & XZ_COMPRESS != 0 {
            let xz_file = File::create(path.join([name, ".xz"].concat()))?;
            writers.push(Box::new(XzEncoder::new(xz_file, 9)));
        }

        writers
    };

    log::info!(
        "compressing {} to {}: uncompressed: {}, gzip: {}, xz: {}",
        name,
        path.display(),
        support & UNCOMPRESSED != 0,
        support & GZ_COMPRESS != 0,
        support & XZ_COMPRESS != 0
    );

    BusWriter::new(stream, &mut destinations, |_| {}, || false).write()?;

    Ok(())
}
