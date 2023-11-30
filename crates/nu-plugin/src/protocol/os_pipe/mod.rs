use std::{io::Write, thread::JoinHandle};

use log::trace;
use nu_protocol::{PipelineData, ShellError, Span, StreamDataType};
pub use pipe_custom_value::StreamCustomValue;
use serde::{Deserialize, Serialize};

use super::CallInput;
mod big_array;
mod encoder;
mod misc;
mod pipe_custom_value;
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(unix, path = "unix.rs")]
mod pipe_impl;

use misc::*;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct OsPipe {
    pub span: Span,
    pub datatype: StreamDataType,

    read_handle: Handle,
    write_handle: Handle,

    handle_policy: HandlePolicy,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum HandlePolicy {
    Exclusive,
    Inclusive,
}

impl OsPipe {
    /// Creates a new pipe. Pipes are unidirectional streams of bytes composed of a read end and a write end. They can be used for interprocess communication.
    /// Uses `pipe(2)` on unix and `CreatePipe` on windows.
    pub fn create(span: Span) -> Result<Self, PipeError> {
        let pipe = pipe_impl::create_pipe(span)?;
        assert!(pipe.write_handle.1 == HandleTypeEnum::Write);
        assert!(pipe.read_handle.1 == HandleTypeEnum::Read);
        Ok(pipe)
    }

    /// Closes the write end of the pipe. This is needed to signal the end of the stream to the reader.
    pub fn close_write(&self) -> Result<(), PipeError> {
        pipe_impl::close_handle(self.write_handle)
    }

    /// Closes the read end of the pipe. This is needed to signal we are done reading from the pipe.
    pub fn close_read(&self) -> Result<(), PipeError> {
        pipe_impl::close_handle(self.read_handle)
    }

    /// Set policy for the pipe handles. If set to `HandlePolicy::Exclusive`, the pipe will close the other end of the pipe when a handle is created.
    pub fn set_handle_policy(&mut self, policy: HandlePolicy) {
        self.handle_policy = policy;
    }

    /// Returns the read end of the pipe.

    /// Returns a `HandleReader` for reading from the pipe.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::protocol::os_pipe::HandleReader;
    ///
    /// let pipe = /* create an instance of the pipe */;
    /// let reader = pipe.reader();
    /// // Use the reader to read from the pipe
    /// ```
    pub fn open_read(&self) -> HandleReader {
        self.on_open_read();
        HandleReader(self.read_handle)
    }

    fn on_open_read(&self) {
        if self.handle_policy == HandlePolicy::Exclusive {
            let _ = self.close_write();
        }
    }

    /// Returns a buffered handle writer for the pipe. Prefer this over `unbuffered_writer` for better performance.
    ///
    /// ### Closing
    /// It is crucial to call `writer.close()` on the writer when you are done with it.
    /// Otherwise the buffered writer will not flush the buffer to the pipe and the reader will hang waiting for more data.
    ///
    /// If you do not want to close the handle but still want to flush the buffer, you can call `writer.flush()` instead.
    pub fn open_write(&self) -> BufferedHandleWriter {
        self.on_open_write();
        BufferedHandleWriter::new(self.write_handle)
    }

    /// Returns an unbuffered writer for the pipe. Prefer `writer` over this for better performance.
    ///
    /// ### Closing
    /// It is crucial to call `writer.close()` on the writer when you are done with it to signal the end of the stream to the reader.
    /// Failing to do so will cause the reader to hang waiting for more data.
    pub fn open_write_unbuffered(&self) -> UnbufferedHandleWriter {
        self.on_open_write();
        UnbufferedHandleWriter(self.write_handle)
    }

    fn on_open_write(&self) {
        if self.handle_policy == HandlePolicy::Exclusive {
            let _ = self.close_read();
        }
    }

    /// Returns a tuple containing a `HandleReader` and a `BufferedHandleWriter`.
    ///
    /// # Example
    ///
    /// ```
    /// use crate::protocol::os_pipe::HandleReader;
    /// use crate::protocol::os_pipe::BufferedHandleWriter;
    ///
    /// let (reader, writer) = rw();
    /// ```
    pub fn rw(&self) -> (HandleReader, BufferedHandleWriter) {
        (self.open_read(), self.open_write())
    }

    /// Returns a tuple containing a `HandleReader` and an `UnbufferedHandleWriter`. Prefer `rw` over this for better performance.
    pub fn urw(&self) -> (HandleReader, UnbufferedHandleWriter) {
        (self.open_read(), self.open_write_unbuffered())
    }
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

/// A struct representing a buffered handle writer. Prefer this over `UnbufferedHandleWriter` for better performance.
#[derive(Debug)]
pub struct BufferedHandleWriter {
    handle: Handle,
    writer: std::io::BufWriter<UnbufferedHandleWriter>,
}

impl BufferedHandleWriter {
    fn new(handle: Handle) -> Self {
        Self {
            handle,
            writer: std::io::BufWriter::new(UnbufferedHandleWriter(handle)),
        }
    }
}

/// Represents an unbuffered handle writer. Prefer `BufferedHandleWriter` over this for better performance.
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
#[repr(transparent)]
pub struct UnbufferedHandleWriter(Handle);

pub trait HasHandle {
    /// Returns the handle of the object.
    fn handle(&self) -> Handle;

    /// Closes the handle of the object.
    #[inline(always)]
    fn close(&mut self) -> Result<(), PipeError> {
        self.handle().close()
    }
}

impl HasHandle for HandleReader {
    fn handle(&self) -> Handle {
        self.0
    }
}

impl HasHandle for BufferedHandleWriter {
    fn handle(&self) -> Handle {
        self.handle
    }

    #[allow(clippy::useless_conversion)]
    fn close(&mut self) -> Result<(), PipeError> {
        self.writer
            .flush()
            .map_err(|e| PipeError::FailedToWrite(self.handle, e.into()))?;
        self.handle.close()
    }
}

impl HasHandle for UnbufferedHandleWriter {
    fn handle(&self) -> Handle {
        self.0
    }
}

impl std::io::Write for UnbufferedHandleWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        pipe_impl::write_handle(self.0, buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl std::io::Write for BufferedHandleWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

/// A struct representing a handle reader.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandleReader(Handle);

impl std::io::Read for HandleReader {
    #[allow(clippy::useless_conversion)]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        pipe_impl::read_handle(self.0, buf).map_err(|e| e.into())
    }
}

impl UnbufferedHandleWriter {
    pub fn buffered(self) -> BufferedHandleWriter {
        BufferedHandleWriter::new(self.0)
    }
}

impl OsPipe {
    /// Starts a new thread that will pipe the stdout stream to the os pipe.
    ///
    /// Returns a handle to the thread if the input is a pipe and the output is an external stream.
    pub fn start_pipe(input: &mut CallInput) -> Result<Option<JoinHandle<()>>, ShellError> {
        match input {
            CallInput::Pipe(os_pipe, Some(PipelineData::ExternalStream { stdout, .. })) => {
                let Some(stdout) = stdout.take() else {
                    return Ok(None);
                };
                let os_pipe = os_pipe.clone();
                let writer = os_pipe.open_write_unbuffered();

                let handle = std::thread::spawn(move || {
                    let mut writer = writer;
                    let stdout = stdout;

                    stdout.stream.for_each(|e| match e {
                        Ok(ref e) => {
                            let written = writer.write(e);
                            match written {
                                Ok(written) => {
                                    if written != e.len() {
                                        trace!(
                                            "OsPipe::start_pipe thread partial write to pipe: \
                                             {} bytes written",
                                            written
                                        );
                                    } else {
                                        trace!(
                                            "OsPipe::start_pipe thread wrote {} bytes to pipe",
                                            written
                                        );
                                    }
                                }
                                Err(e) => {
                                    trace!(
                                        "OsPipe::start_pipe thread error: failed to write to \
                                         pipe: {:?}",
                                        e
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            trace!("OsPipe::start_pipe thread error: {:?}", e);
                        }
                    });
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

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    fn test_pipe() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        pipe.set_handle_policy(HandlePolicy::Inclusive);
        println!("{:?}", pipe);
        let (mut reader, mut writer) = pipe.urw();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();
        pipe.close_write().unwrap();

        assert_eq!(written, 11);

        let mut buf = [0u8; 256];

        let read = reader.read(&mut buf).unwrap();
        pipe.close_read().unwrap();

        assert_eq!(read, 11);
        assert_eq!(&buf[..read], "hello world".as_bytes());
    }

    #[test]
    fn test_serialized_pipe() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        pipe.set_handle_policy(HandlePolicy::Inclusive);
        let mut writer = pipe.open_write_unbuffered();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        writer.close().unwrap();

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
        println!("{}", serialized);
        // deserialize the pipe
        let deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();
        let mut reader = deserialized.open_read();

        let mut buf = [0u8; 11];

        let read = reader.read(&mut buf).unwrap();

        assert_eq!(read, 11);
        assert_eq!(buf, "hello world".as_bytes());
        reader.close().unwrap();
    }

    #[test]
    fn test_pipe_in_another_thread() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        pipe.set_handle_policy(HandlePolicy::Inclusive);
        let mut writer = pipe.open_write_unbuffered();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);
        writer.close().unwrap();

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
        // spawn a new process
        std::thread::spawn(move || {
            // deserialize the pipe
            let deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();
            let mut reader = deserialized.open_read();

            let mut buf = [0u8; 11];

            let read = reader.read(&mut buf).unwrap();

            assert_eq!(read, 11);
            assert_eq!(buf, "hello world".as_bytes());
            reader.close().unwrap();
        });
    }

    #[test]
    fn test_pipe_in_another_process() {
        let pipe = OsPipe::create(Span::unknown()).unwrap();
        let mut writer = pipe.open_write_unbuffered();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);
        writer.close().unwrap();

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
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
