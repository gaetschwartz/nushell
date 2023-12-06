use serde::{Deserialize, Serialize};

use crate::errors::{PipeError, PipeResult};

use self::{
    pipe_impl::InnerHandleType,
    unidirectional::{Pipe, PipeMode, PipeRead, PipeWrite},
};

// pub mod bidirectional;
pub mod unidirectional;

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(unix, path = "unix.rs")]
mod pipe_impl;

pub use pipe_impl::OSError;

const BUFFER_CAPACITY: usize = 16 * 1024 * 1024;
const ZSTD_COMPRESSION_LEVEL: i32 = 0;

trait PipeImplBase {
    fn create_pipe() -> Result<OsPipe, PipeError>;

    fn close_handle(handle: &Handle) -> PipeResult<()>;

    fn read_handle(handle: &Handle, buf: &mut [u8]) -> PipeResult<usize>;

    fn write_handle(handle: &Handle, buf: &[u8]) -> PipeResult<usize>;

    fn should_close_other_for_mode(mode: PipeMode) -> bool;
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub(crate) struct OsPipe {
    read_handle: Handle,
    write_handle: Handle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HandleTypeEnum {
    #[serde(rename = "read")]
    Read,
    #[serde(rename = "write")]
    Write,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Handle(
    #[cfg_attr(windows, serde(with = "pipe_impl::handle_serialization"))] InnerHandleType,
    HandleTypeEnum,
);

impl Handle {
    pub fn close(&self) -> Result<(), PipeError> {
        pipe_impl::PipeImpl::close_handle(self)
    }

    #[allow(non_snake_case)]
    #[inline(always)]
    fn Read(handle: InnerHandleType) -> Handle {
        Handle(handle, HandleTypeEnum::Read)
    }

    #[allow(non_snake_case)]
    #[inline(always)]
    fn Write(handle: InnerHandleType) -> Handle {
        Handle(handle, HandleTypeEnum::Write)
    }

    #[inline(always)]
    fn native(&self) -> InnerHandleType {
        self.0
    }
}

impl From<Handle> for InnerHandleType {
    fn from(val: Handle) -> Self {
        val.0
    }
}

/// Represents an unbuffered handle writer. Prefer `BufferedHandleWriter` over this for better performance.
pub struct HandleWriter<'p> {
    pipe: &'p Pipe<PipeWrite>,
    writer: Option<Box<dyn FinishableWrite<Inner = &'p Pipe<PipeWrite>> + 'p>>,
}

impl<'p> HandleWriter<'p> {
    pub fn new<'o: 'p>(pipe: &'o Pipe<PipeWrite>) -> Self {
        Self {
            pipe,
            writer: Some(match pipe.encoding() {
                StreamEncoding::Zstd => Box::new(
                    zstd::stream::Encoder::new(pipe, ZSTD_COMPRESSION_LEVEL)
                        .expect("failed to create zstd encoder"),
                ),
                StreamEncoding::None => Box::new(pipe),
            }),
        }
    }
}

impl std::io::Write for HandleWriter<'_> {
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

    fn finish(self: Box<Self>) -> Result<(), std::io::Error>;
}

impl<'a> FinishableWrite for zstd::stream::Encoder<'a, &'a Pipe<PipeWrite>> {
    fn finish(self: Box<Self>) -> Result<(), std::io::Error> {
        zstd::stream::Encoder::finish(*self)?;
        Ok(())
    }

    type Inner = &'a Pipe<PipeWrite>;
}

impl<'p> FinishableWrite for &'p Pipe<PipeWrite> {
    type Inner = &'p Pipe<PipeWrite>;

    #[inline(always)]
    fn finish(self: Box<Self>) -> Result<(), std::io::Error> {
        Ok(())
    }
}

/// A struct representing a handle reader.
pub struct HandleReader<'p> {
    reader: Box<dyn std::io::Read + 'p>,
    pipe: &'p Pipe<PipeRead>,
}

impl<'p> HandleReader<'p> {
    pub fn new<'o: 'p>(pipe: &'o Pipe<PipeRead>) -> Self {
        Self {
            pipe,
            reader: match pipe.encoding() {
                StreamEncoding::Zstd => {
                    if let Ok(decoder) = zstd::stream::Decoder::new(pipe) {
                        Box::new(decoder)
                    } else {
                        eprintln!("failed to create zstd decoder, falling back to raw");
                        Box::new(std::io::BufReader::with_capacity(BUFFER_CAPACITY, pipe))
                    }
                }
                StreamEncoding::None => {
                    Box::new(std::io::BufReader::with_capacity(BUFFER_CAPACITY, pipe))
                }
            },
        }
    }
}

impl std::io::Read for HandleReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

pub trait HandleIO {
    /// Returns the handle of the object.
    fn handle(&self) -> Handle;
    fn encoding(&self) -> StreamEncoding;
}

pub trait AsNativeHandle {
    /// Returns the native handle of the object.
    fn as_native_handle(&self) -> InnerHandleType;
}

impl AsNativeHandle for Handle {
    fn as_native_handle(&self) -> InnerHandleType {
        self.native()
    }
}

impl<T: HandleIO> AsNativeHandle for T {
    fn as_native_handle(&self) -> InnerHandleType {
        self.handle().native()
    }
}

impl HandleIO for HandleWriter<'_> {
    fn handle(&self) -> Handle {
        self.pipe.handle
    }

    fn encoding(&self) -> StreamEncoding {
        self.pipe.encoding
    }
}

impl HandleIO for HandleReader<'_> {
    fn handle(&self) -> Handle {
        self.pipe.handle
    }

    fn encoding(&self) -> StreamEncoding {
        self.pipe.encoding
    }
}

pub trait Closeable: HandleIO {
    /// Closes the object.
    fn close(&mut self) -> Result<(), std::io::Error>;
}

impl Closeable for HandleWriter<'_> {
    fn close(&mut self) -> Result<(), std::io::Error> {
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

impl Closeable for HandleReader<'_> {
    fn close(&mut self) -> Result<(), std::io::Error> {
        self.pipe.close().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to close handle: {:?}", e),
            )
        })
    }
}

impl std::fmt::Display for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(windows)]
        let handle = self.0 .0;
        #[cfg(unix)]
        let handle = self.0;
        write!(f, "{:?} ({})", self.1, handle)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StreamEncoding {
    Zstd,
    None,
}
