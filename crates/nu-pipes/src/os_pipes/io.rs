use std::io::{BufReader, BufWriter, Write};

use nu_protocol::ShellError;

use crate::{
    errors::PipeError,
    unidirectional::{PipeRead, PipeWrite, RawPipeReader, RawPipeWriter},
    PipeFd, PIPE_BUFFER_CAPACITY,
};

/// Represents an unbuffered handle writer. Prefer `BufferedHandleWriter` over this for better performance.
pub struct PipeWriter<'a> {
    pub(crate) fd: &'a PipeFd<PipeWrite>,
    writer: BufWriter<RawPipeWriter<&'a PipeFd<PipeWrite>>>,
}

impl<'a> PipeWriter<'a> {
    pub fn new<'b: 'a>(fd: &'b PipeFd<PipeWrite>) -> Self {
        let writer = BufWriter::with_capacity(PIPE_BUFFER_CAPACITY, RawPipeWriter(fd));
        Self { fd, writer }
    }
}

impl std::io::Write for PipeWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

/// A struct representing a handle reader.
pub struct PipeReader<'a> {
    pub fd: &'a PipeFd<PipeRead>,
    pub(crate) reader: BufReader<RawPipeReader<&'a PipeFd<PipeRead>>>,
}

impl<'a> PipeReader<'a> {
    pub fn new<'b: 'a>(fd: &'b PipeFd<PipeRead>) -> Self {
        let reader = BufReader::with_capacity(PIPE_BUFFER_CAPACITY, RawPipeReader(fd));

        Self { reader, fd }
    }
}

impl std::fmt::Debug for PipeReader<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipeReader").field("fd", &self.fd).finish()
    }
}

impl std::io::Read for PipeReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

pub struct OwningPipeReader {
    pub(crate) reader: BufReader<RawPipeReader<PipeFd<PipeRead>>>,
}

impl OwningPipeReader {
    pub fn new(fd: PipeFd<PipeRead>) -> Self {
        let reader = BufReader::with_capacity(PIPE_BUFFER_CAPACITY, RawPipeReader(fd));

        Self { reader }
    }

    pub fn close(self) -> Result<(), CloseOwningError<OwningPipeReader, PipeError>> {
        let fd = unsafe { std::ptr::read(&self.reader.get_ref().0) };

        match fd.close() {
            Ok(_) => Ok(()),
            Err(e) => {
                let (err, _) = e.into_parts();
                Err(CloseOwningError::new(err, self))
            }
        }
    }

    fn fd(&self) -> &PipeFd<PipeRead> {
        &self.reader.get_ref().0
    }

    pub fn into_inner(self) -> PipeFd<PipeRead> {
        self.reader.into_inner().0
    }
}

impl std::fmt::Debug for OwningPipeReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwningPipeReader")
            .field("fd", self.fd())
            .finish()
    }
}

impl std::io::Read for OwningPipeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

pub struct OwningPipeWriter {
    pub(crate) writer: BufWriter<RawPipeWriter<PipeFd<PipeWrite>>>,
}

impl OwningPipeWriter {
    pub fn new(fd: PipeFd<PipeWrite>) -> Self {
        let writer = BufWriter::with_capacity(PIPE_BUFFER_CAPACITY, RawPipeWriter(fd));

        Self { writer }
    }

    fn fd(&self) -> &PipeFd<PipeWrite> {
        &self.writer.get_ref().0
    }

    pub fn into_inner(
        mut self,
    ) -> Result<PipeFd<PipeWrite>, CloseOwningError<OwningPipeWriter, std::io::Error>> {
        match self.writer.into_inner() {
            Ok(writer) => Ok(writer.0),
            Err(e) => {
                let (err, writer) = e.into_parts();
                self.writer = writer;
                Err(CloseOwningError::new(err, self))
            }
        }
    }

    pub fn close(mut self) -> Result<(), CloseOwningError<OwningPipeWriter, std::io::Error>> {
        if let Err(e) = self.flush() {
            return Err(CloseOwningError::new(e, self));
        }

        let fd = unsafe { std::ptr::read(&self.writer.get_ref().0) };

        match fd.close() {
            Ok(_) => Ok(()),
            Err(e) => {
                let (err, _) = e.into_parts();

                Err(CloseOwningError::new(err.into(), self))
            }
        }
    }
}

impl std::fmt::Debug for OwningPipeWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwningPipeWriter")
            .field("fd", self.fd())
            .finish()
    }
}

impl std::io::Write for OwningPipeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

#[derive(Debug)]
pub struct CloseOwningError<T, E: std::error::Error> {
    error: E,
    inner: T,
}

impl<T, E: std::error::Error> CloseOwningError<T, E> {
    pub fn new(error: E, inner: T) -> Self {
        Self { error, inner }
    }

    pub fn from_parts<U, F>((error, inner): (F, U)) -> Self
    where
        F: Into<E>,
        U: Into<T>,
    {
        Self {
            error: error.into(),
            inner: inner.into(),
        }
    }

    pub fn error(&self) -> &E {
        &self.error
    }

    pub fn into_parts(self) -> (E, T) {
        (self.error, self.inner)
    }

    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T> From<CloseOwningError<T, std::io::Error>> for std::io::Error {
    fn from(e: CloseOwningError<T, std::io::Error>) -> Self {
        e.error
    }
}
impl<T> From<CloseOwningError<T, PipeError>> for PipeError {
    fn from(e: CloseOwningError<T, PipeError>) -> Self {
        e.error
    }
}
impl<T> From<CloseOwningError<T, PipeError>> for ShellError {
    fn from(e: CloseOwningError<T, PipeError>) -> Self {
        e.error.into()
    }
}
