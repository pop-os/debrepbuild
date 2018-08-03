use bus_writer::BusWriter;
use deflate::Compression;
use deflate::write::GzEncoder;
use xz2::write::XzEncoder;
use std::io;
use std::path::Path;
use std::fs::File;

pub const UNCOMPRESSED: u8 = 1;
pub const GZ_COMPRESS: u8 = 2;
pub const XZ_COMPRESS: u8 = 4;

pub trait SyncWrite: Send + Sync + io::Write {}
impl<T: Send + Sync + io::Write> SyncWrite for T {}

pub fn compress<R: io::Read>(name: &str, path: &Path, stream: R, support: u8) -> io::Result<()> {
    if support == 0 {
        return Ok(());
    }

    let mut destinations = {
        let mut writers: Vec<Box<SyncWrite>> = Vec::new();
        if support & UNCOMPRESSED != 0 {
            writers.push(Box::new(File::create(path.join(name))?));
        }

        if support & GZ_COMPRESS != 0 {
            let mut gz_file = File::create(path.join([name, ".gz"].concat()))?;
            writers.push(Box::new(GzEncoder::new(gz_file, Compression::Best)));
        }

        if support & XZ_COMPRESS != 0 {
            let mut xz_file = File::create(path.join([name, ".xz"].concat()))?;
            writers.push(Box::new(XzEncoder::new(xz_file, 9)));
        }

        writers
    };

    info!(
        "compressing to {}: uncompressed: {}, gzip: {}, xz: {}",
        path.display(),
        support & UNCOMPRESSED != 0,
        support & GZ_COMPRESS != 0,
        support & XZ_COMPRESS != 0
    );

    BusWriter::new(stream, &mut destinations, |_| {}, || false).write()
}