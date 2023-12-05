use std::thread::JoinHandle;

use log::trace;
use nu_protocol::ShellError;
use serde::{Deserialize, Serialize};

use crate::{InnerHandleType, MaybeRawStream, PipeError};

use self::unidirectional::{Pipe, PipeRead, PipeWrite, UnOpenedPipe};

// pub mod bidirectional;
pub mod unidirectional;

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(unix, path = "unix.rs")]
pub mod pipe_impl;

const BUFFER_CAPACITY: usize = 16 * 1024 * 1024;
const ZSTD_COMPRESSION_LEVEL: i32 = 0;

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
    pub fn close(self) -> Result<(), PipeError> {
        pipe_impl::close_handle(self)
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

impl UnOpenedPipe<PipeWrite> {
    /// Starts a new thread that will pipe the stdout stream to the os pipe.
    ///
    /// Returns a handle to the thread if the input is a pipe and the output is an external stream.
    pub fn send(
        &self,
        input: &mut impl MaybeRawStream,
    ) -> Result<Option<JoinHandle<()>>, ShellError> {
        match input.take_stream() {
            Some(stdout) => {
                let pipe = self.open().unwrap();

                let handle = std::thread::spawn(move || {
                    let pipe = pipe;
                    let mut writer = HandleWriter::new(&pipe);
                    let mut stdout = stdout;

                    match std::io::copy(&mut stdout, &mut writer) {
                        Ok(_) => {
                            trace!("OsPipe::start_pipe thread finished writing");
                        }
                        Err(e) => {
                            trace!(
                                "OsPipe::start_pipe thread error: failed to write to pipe: {:?}",
                                e
                            );
                        }
                    }

                    match writer.close() {
                        Ok(_) => {
                            trace!("OsPipe::start_pipe thread flushed pipe");
                        }
                        Err(e) => {
                            trace!(
                                "OsPipe::start_pipe thread error: failed to flush pipe: {:?}",
                                e
                            );
                        }
                    }

                    trace!("OsPipe::start_pipe thread finished writing, closing pipe");
                    // close the pipe when the stream is finished
                });

                Ok(Some(handle))
            }
            _ => Ok(None),
        }
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

#[cfg(test)]
mod tests {

    use std::io::{Read, Write};

    use crate::unidirectional::{
        PipeMode, PipeRead, UnOpenedPipe, UniDirectionalPipeOptions, UnidirectionalPipe,
    };

    use super::*;

    impl UnidirectionalPipe {
        pub fn in_process() -> Self {
            Self::create_from_options(UniDirectionalPipeOptions {
                encoding: StreamEncoding::None,
                mode: PipeMode::InProcess,
            })
            .unwrap()
        }
    }

    #[test]
    fn test_pipe() {
        let UnidirectionalPipe { read, write } = UnidirectionalPipe::in_process();
        let mut reader = read.open().unwrap();
        let mut writer = write.open().unwrap();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();
        writer.close().unwrap();

        assert_eq!(written, 11);

        let mut buf = [0u8; 256];

        let read = reader.read(&mut buf).unwrap();
        reader.close().unwrap();

        assert_eq!(read, 11);
        assert_eq!(&buf[..read], "hello world".as_bytes());
    }

    #[test]
    fn test_serialized_pipe() {
        let UnidirectionalPipe { read, write } = UnidirectionalPipe::in_process();
        let mut writer = write.open().unwrap();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        writer.close().unwrap();

        // serialize the pipe
        let serialized = serde_json::to_string(&read).unwrap();
        println!("{}", serialized);
        // deserialize the pipe
        let deserialized: UnOpenedPipe<PipeRead> = serde_json::from_str(&serialized).unwrap();
        let mut reader = deserialized.open().unwrap();

        let mut buf = [0u8; 11];

        let read = reader.read(&mut buf).unwrap();

        assert_eq!(read, 11);
        assert_eq!(buf, "hello world".as_bytes());
        reader.close().unwrap();
    }

    #[test]
    fn test_pipe_in_another_thread() {
        let UnidirectionalPipe { read, write } = UnidirectionalPipe::in_process();
        let mut writer = write.open().unwrap();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);
        writer.close().unwrap();

        // serialize the pipe
        let serialized = serde_json::to_string(&read).unwrap();
        // spawn a new process
        let (read, buf) = std::thread::spawn(move || {
            // deserialize the pipe
            let deserialized: UnOpenedPipe<PipeRead> = serde_json::from_str(&serialized).unwrap();
            let mut reader = deserialized.open().unwrap();

            let mut buf = [0u8; 32];

            let read = reader.read(&mut buf).unwrap();

            reader.close().unwrap();

            (read, buf)
        })
        .join()
        .unwrap();

        assert_eq!(read, 11);
        assert_eq!(&buf[..read], "hello world".as_bytes());
    }

    #[test]
    fn test_pipe_in_another_process() {
        let UnidirectionalPipe { read, write } =
            UnidirectionalPipe::create_from_options(UniDirectionalPipeOptions {
                encoding: StreamEncoding::None,
                mode: PipeMode::CrossProcess,
            })
            .unwrap();

        let mut writer = write.open().unwrap();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);
        writer.close().unwrap();

        // serialize the pipe
        let serialized = serde_json::to_string(&read).unwrap();
        println!("{}", serialized);
        // spawn a new process
        let res = std::process::Command::new("cargo")
            .arg("run")
            .arg("-q")
            .arg("--bin")
            .arg("nu_plugin_pipe_echoer")
            .arg(serialized)
            .output()
            .unwrap();

        if !res.status.success() {
            panic!("stderr: {}", String::from_utf8_lossy(res.stderr.as_slice()));
        }

        assert!(res.status.success());
        assert_eq!(
            String::from_utf8_lossy(res.stdout.as_slice()),
            "hello world\n"
        );
    }
}
