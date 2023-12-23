//! This module contains the implementation of pipe I/O for reading and writing data.
//!
//! It provides the following structs:
//! - `PipeWriter`: A struct for writing data to a pipe.
//! - `PipeReader`: A struct for reading data from a pipe.
//! - `OwningPipeReader`: A struct for owning a pipe reader.
//! - `OwningPipeWriter`: A struct for owning a pipe writer.
//! - `CloseOwningError`: An error type for handling errors when closing owning pipe readers or writers.
//! - `PipeIterator`: An iterator for reading data from a pipe.
//!
//! These structs are used to perform I/O operations on pipes, which are used for inter-process communication.
//! The `PipeWriter` and `PipeReader` structs wrap the `BufWriter` and `BufReader` types from the `std::io` module,
//! providing a convenient interface for writing and reading data to and from pipes.
//! The `OwningPipeReader` and `OwningPipeWriter` structs are used to own the pipe reader and writer, respectively,
//! and provide additional functionality for closing the pipe and accessing the underlying file descriptor.
//! The `CloseOwningError` type is used to handle errors that occur when closing owning pipe readers or writers.
//! The `PipeIterator` struct is an iterator that reads data from a pipe, returning it as a vector of bytes.
//! It can be used to easily iterate over the data read from a pipe.
//!
//! This module is used in the `nu-pipes` crate for handling pipe I/O operations.
use std::io::{BufReader, BufWriter, Write};

use nu_protocol::ShellError;

use crate::{
    errors::PipeError,
    unidirectional::{PipeRead, PipeWrite, RawPipeReader, RawPipeWriter},
    PipeFd, PIPE_BUFFER_CAPACITY,
};

/// A structure representing a pipe writer.
///
/// This structure is used to write data to a pipe.
pub struct PipeWriter<'a> {
    pub(crate) fd: &'a PipeFd<PipeWrite>,
    writer: BufWriter<RawPipeWriter<&'a PipeFd<PipeWrite>>>,
}

impl<'a> PipeWriter<'a> {
    /// Creates a new `PipeWriter` instance.
    ///
    /// # Arguments
    ///
    /// * `fd` - A reference to a `PipeFd<PipeWrite>` object representing the pipe file descriptor.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nu_pipes::io::PipeWriter;
    /// use nu_pipes::{PipeFd, FromRawPipeFd};
    ///
    /// let pipe_fd = unsafe { PipeFd::from_raw_pipe_fd(0) };
    /// let writer = PipeWriter::new(&pipe_fd);
    /// ```
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

/// Represents a pipe reader.
///
/// This struct is used to read data from a pipe.
pub struct PipeReader<'a> {
    /// The pipe file descriptor.
    pub fd: &'a PipeFd<PipeRead>,
    pub(crate) reader: BufReader<RawPipeReader<&'a PipeFd<PipeRead>>>,
}

impl<'a> PipeReader<'a> {
    /// Creates a new `PipeReader` instance.
    ///
    /// # Arguments
    ///
    /// * `fd` - A reference to a `PipeFd` representing the pipe file descriptor.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nu_pipes::io::PipeReader;
    /// use nu_pipes::{PipeFd, FromRawPipeFd};
    ///
    /// let pipe_fd = unsafe { PipeFd::from_raw_pipe_fd(0) };
    /// let reader = PipeReader::new(&pipe_fd);
    /// ```
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

impl std::io::BufRead for PipeReader<'_> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.reader.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.reader.consume(amt)
    }
}

/// A reader owning the underlying pipe file descriptor. See `PipeReader` for more information.
///
/// This struct is used to read data from a pipe.
pub struct OwningPipeReader {
    pub(crate) reader: BufReader<RawPipeReader<PipeFd<PipeRead>>>,
}

impl OwningPipeReader {
    /// Creates a new `OwningPipeReader` with the given pipe file descriptor.
    ///
    /// # Arguments
    ///
    /// * `fd` - The pipe file descriptor.
    ///
    /// # Returns
    ///
    /// A new `OwningPipeReader` instance.
    pub fn new(fd: PipeFd<PipeRead>) -> Self {
        let reader = BufReader::with_capacity(PIPE_BUFFER_CAPACITY, RawPipeReader(fd));

        Self { reader }
    }

    /// Closes the `OwningPipeReader` and releases the underlying file descriptor.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the pipe is successfully closed.
    /// - `Err(CloseOwningError)` if an error occurs while closing the pipe.
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

    /// Returns a reference to the underlying pipe file descriptor.
    pub fn fd(&self) -> &PipeFd<PipeRead> {
        &self.reader.get_ref().0
    }

    /// Consumes the `OwningPipeReader` and returns the underlying pipe file descriptor.
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
impl std::io::BufRead for OwningPipeReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.reader.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.reader.consume(amt)
    }
}

/// A struct representing an owning pipe writer.
///
/// This struct is used to write data to a pipe. It contains a buffered writer
/// that writes data to the underlying pipe file descriptor.
pub struct OwningPipeWriter {
    pub(crate) writer: BufWriter<RawPipeWriter<PipeFd<PipeWrite>>>,
}

impl OwningPipeWriter {
    /// Creates a new `OwningPipeWriter` with the given pipe file descriptor.
    ///
    /// # Arguments
    ///
    /// * `fd` - The pipe file descriptor to write to.
    ///
    /// # Returns
    ///
    /// A new `OwningPipeWriter` instance.
    pub fn new(fd: PipeFd<PipeWrite>) -> Self {
        Self {
            writer: BufWriter::with_capacity(PIPE_BUFFER_CAPACITY, RawPipeWriter(fd)),
        }
    }

    /// Returns a reference to the underlying pipe file descriptor.
    pub fn fd(&self) -> &PipeFd<PipeWrite> {
        &self.writer.get_ref().0
    }

    /// Consumes the `OwningPipeWriter` and returns the underlying pipe file descriptor.  
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

    /// Closes the `OwningPipeWriter` and releases the underlying file descriptor.
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

/// A wrapper type for an error that occurred when closing the owning pipe reader or writer.
///
/// This type is used to associate an error with the underlying resource that failed to close.
/// It provides methods to access the error and the underlying resource.
///
/// # Type Parameters
///
/// - `T`: The type of the underlying resource.
/// - `E`: The type of the error that occurred when closing the resource.
///
/// # Examples
///
/// ```no_run
/// use std::io::Error;
/// use nu_pipes::io::CloseOwningError;
///
/// let error = Error::new(std::io::ErrorKind::Other, "Failed to close pipe");
/// let resource = 42;
///
/// let close_error = CloseOwningError::new(error, resource);
///
/// assert_eq!(close_error.error().kind(), std::io::ErrorKind::Other);
/// assert_eq!(*close_error.get_ref(), 42);
/// ```
#[derive(Debug)]
pub struct CloseOwningError<T, E: std::error::Error> {
    error: E,
    inner: T,
}

impl<T, E: std::error::Error> CloseOwningError<T, E> {
    /// Creates a new `CloseOwningError` instance.
    pub fn new(error: E, inner: T) -> Self {
        Self { error, inner }
    }

    /// A reference to the error that occurred when closing the owning pipe reader or writer.
    pub fn error(&self) -> &E {
        &self.error
    }

    /// Consumes the `CloseOwningError` and returns the error and the underlying resource.
    pub fn into_parts(self) -> (E, T) {
        (self.error, self.inner)
    }

    /// Returns a reference to the underlying resource.
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    /// Returns a mutable reference to the underlying resource.
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

#[derive(Debug)]
enum MaybeOwnedMut<'a> {
    Owned(&'a mut OwningPipeReader),
    Borrowed(&'a mut PipeReader<'a>),
}

#[derive(Debug)]
/// Represents an iterator over a pipe, providing access to the data read from the pipe.
pub struct PipeIterator<'a> {
    reader: MaybeOwnedMut<'a>,
}

impl Iterator for PipeIterator<'_> {
    type Item = Result<Vec<u8>, ShellError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = vec![0u8; PIPE_BUFFER_CAPACITY];

        let reader: &mut dyn std::io::Read = match &mut self.reader {
            MaybeOwnedMut::Owned(reader) => reader,
            MaybeOwnedMut::Borrowed(reader) => reader,
        };

        match reader.read(&mut buf) {
            Ok(0) => None,
            Ok(_) => Some(Ok(buf)),
            Err(e) => Some(Err(e.into())),
        }
    }
}

impl<'a> PipeReader<'a> {
    /// Returns an iterator over the data read from the pipe.
    ///
    /// The iterator yields chunks of data as they become available from the pipe.
    /// This method is used to create a `PipeIterator` from a `PipeReader`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nu_pipes::io::{PipeReader, PipeIterator};
    /// use nu_pipes::{PipeFd, FromRawPipeFd};
    ///
    /// let pipe_fd = unsafe { PipeFd::from_raw_pipe_fd(0) };
    /// let mut reader = PipeReader::new(&pipe_fd);
    /// let mut iterator = reader.stream();
    ///
    /// // Read data from the pipe using the iterator
    /// for chunk in iterator {
    ///     // Process the chunk of data
    ///     println!("{:?}", chunk);
    /// }
    /// ```
    pub fn stream(&'a mut self) -> PipeIterator {
        PipeIterator {
            reader: MaybeOwnedMut::Borrowed(self),
        }
    }
}

impl OwningPipeReader {
    /// Returns an iterator over the data read from the owning pipe reader.
    ///
    /// The iterator yields chunks of data as they become available from the pipe.
    /// This method takes ownership of the `OwningPipeReader` and returns a `PipeIterator`.
    /// The iterator can be used to read data from the pipe in a streaming fashion.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nu_pipes::{PipeFd, io::OwningPipeReader, FromRawPipeFd};
    ///
    /// let mut fd = unsafe { PipeFd::from_raw_pipe_fd(0) };
    /// let mut reader = OwningPipeReader::new(fd);
    /// let mut stream = reader.stream();
    ///
    /// // Read data from the pipe in a streaming fashion
    /// for chunk in stream {
    ///     println!("{:?}", chunk);
    /// }
    /// ```
    pub fn stream(&mut self) -> PipeIterator {
        PipeIterator {
            reader: MaybeOwnedMut::Owned(self),
        }
    }
}
