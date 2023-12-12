use std::io::{BufReader, BufWriter, Write};

use nu_protocol::StreamDataType;

use crate::unidirectional::{Pipe, PipeRead, PipeWrite};

pub const PIPE_BUFFER_CAPACITY: usize = 128 * 1024;

/// Represents an unbuffered handle writer. Prefer `BufferedHandleWriter` over this for better performance.
pub struct PipeWriter {
    pub(crate) pipe: Pipe<PipeWrite>,
    writer: Option<BufWriter<Pipe<PipeWrite>>>,
}

impl PipeWriter {
    pub fn new(pipe: Pipe<PipeWrite>) -> Self {
        let finishable_write = BufWriter::with_capacity(PIPE_BUFFER_CAPACITY, pipe.clone());
        Self {
            pipe,
            writer: Some(finishable_write),
        }
    }

    pub fn set_pledged_src_size(&mut self, size: Option<u64>) -> Result<(), std::io::Error> {
        self.writer.as_mut().map_or(
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "writer is already closed",
            )),
            |w| w.set_pledged_src_size(size),
        )
    }

    pub fn close(&mut self) -> Result<(), std::io::Error> {
        let writer = self.writer.take();
        match writer {
            Some(writer) => {
                writer.finish()?;
            }
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "failed to close handle: writer is already closed",
                ))
            }
        }

        self.pipe.close().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to close handle: {:?}", e),
            )
        })
    }
}

impl std::io::Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.as_mut().map_or(
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "writer is already closed",
            )),
            |w| w.write(buf),
        )
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.as_mut().map_or(
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "writer is already closed",
            )),
            |w| w.flush(),
        )
    }
}

trait FinishableWrite: std::io::Write {
    type Inner: Sized;

    fn finish(self) -> Result<(), std::io::Error>;

    fn set_pledged_src_size(&mut self, _size: Option<u64>) -> Result<(), std::io::Error> {
        Ok(())
    }
}

impl FinishableWrite for Pipe<PipeWrite> {
    type Inner = Pipe<PipeWrite>;

    #[inline(always)]
    fn finish(self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

impl<W: FinishableWrite> FinishableWrite for BufWriter<W> {
    fn finish(mut self) -> Result<(), std::io::Error> {
        self.flush()?;
        Box::new(self.into_inner()?).finish()
    }

    fn set_pledged_src_size(&mut self, size: Option<u64>) -> Result<(), std::io::Error> {
        self.get_mut().set_pledged_src_size(size)
    }

    type Inner = W::Inner;
}

/// A struct representing a handle reader.
pub struct PipeReader {
    pub(crate) reader: BufReader<Pipe<PipeRead>>,
    pub pipe: Pipe<PipeRead>,
}

impl Clone for PipeReader {
    fn clone(&self) -> Self {
        PipeReader::new(self.pipe.clone())
    }
}

impl std::fmt::Debug for PipeReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipeReader")
            .field("pipe", &self.pipe)
            .finish()
    }
}

impl PipeReader {
    pub fn new(pipe: Pipe<PipeRead>) -> Self {
        let reader = BufReader::with_capacity(PIPE_BUFFER_CAPACITY, pipe.clone());

        Self { reader, pipe }
    }

    pub fn close(&mut self) -> Result<(), std::io::Error> {
        self.pipe.close().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to close handle: {:?}", e),
            )
        })
    }

    pub fn pipe(&self) -> &Pipe<PipeRead> {
        &self.pipe
    }

    pub fn data_type(&self) -> StreamDataType {
        self.pipe.datatype
    }
}

impl std::io::Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

unsafe impl Sync for PipeReader {}
