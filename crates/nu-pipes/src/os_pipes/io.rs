use std::io::{BufReader, BufWriter, Write};

use nu_protocol::StreamDataType;

use crate::{
    unidirectional::{Pipe, PipeRead, PipeWrite},
    PIPE_BUFFER_CAPACITY,
};

/// Represents an unbuffered handle writer. Prefer `BufferedHandleWriter` over this for better performance.
pub struct PipeWriter {
    pub(crate) pipe: Pipe<PipeWrite>,
    writer: BufWriter<Pipe<PipeWrite>>,
}

impl PipeWriter {
    pub fn new(pipe: Pipe<PipeWrite>) -> Self {
        let finishable_write = BufWriter::with_capacity(PIPE_BUFFER_CAPACITY, pipe.clone());
        Self {
            pipe,
            writer: finishable_write,
        }
    }

    pub fn close(&mut self) -> Result<(), std::io::Error> {
        self.flush()?;

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
        self.writer.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
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
