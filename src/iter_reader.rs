use std::io;

pub struct IteratorReader<T> {
    buffer: Vec<u8>,
    data: T,
}

impl<T: Iterator<Item = Vec<u8>>> IteratorReader<T> {
    pub fn new(data: T, buffer: Vec<u8>) -> Self {
        IteratorReader { buffer, data }
    }
}

impl<T: Iterator<Item = Vec<u8>>> io::Read for IteratorReader<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.buffer.is_empty() {
            while self.buffer.len() < buf.len() {
                match self.data.next() {
                    Some(data) => self.buffer.extend_from_slice(&data),
                    None => break,
                }
            }

            if self.buffer.is_empty() {
                return Ok(0);
            }
        }

        let to_write = self.buffer.len().min(buf.len());
        buf[..to_write].copy_from_slice(&self.buffer[..to_write]);
        if to_write != self.buffer.len() {
            let leftovers = self.buffer.len() - to_write;
            if self.buffer.capacity() < leftovers {
                let reserve = self.buffer.capacity() - leftovers;
                self.buffer.reserve_exact(reserve);
            }

            for (new, old) in (to_write..self.buffer.len()).enumerate() {
                self.buffer[new] = self.buffer[old];
            }

            self.buffer.truncate(leftovers);
        } else {
            self.buffer.clear();
        }

        Ok(to_write)
    }
}
