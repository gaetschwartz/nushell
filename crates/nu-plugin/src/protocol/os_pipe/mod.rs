#[cfg(unix)]
use std::process::Command;
use std::{
    io::{Read, Write},
    thread::JoinHandle,
};

use log::trace;
use nu_protocol::{PipelineData, ShellError, Span, StreamDataType};
pub use pipe_custom_value::StreamCustomValue;
use serde::{Deserialize, Serialize};

trait OsPipeTrait: Read + Write + Send + Sync + Serialize + Deserialize<'static> {
    fn create(span: Span) -> Result<Self, PipeError>;
    fn close(&mut self, handle: Handle) -> Result<(), PipeError>;
}

use super::CallInput;
mod pipe_custom_value;
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(unix, path = "unix.rs")]
mod pipe_impl;

#[derive(Debug)]
pub enum PipeError {
    UnexpectedInvalidPipeHandle,
    FailedToCreatePipe(OSError),
    UnsupportedPlatform,
    FailedToCloseHandle(Handle, OSError),
    FailedToRead(Handle, std::io::Error),
    FailedToWrite(Handle, std::io::Error),
    FailedSetNamedPipeHandleState(Handle, OSError),
}

type PipeResult<T> = Result<T, PipeError>;

#[cfg(windows)]
type InnerHandleType = windows::Win32::Foundation::HANDLE;
#[cfg(unix)]
type InnerHandleType = libc::c_int;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HandleTypeEnum {
    Read,
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
    fn Read(handle: InnerHandleType) -> Handle {
        Handle(handle, HandleTypeEnum::Read)
    }

    #[allow(non_snake_case)]
    fn Write(handle: InnerHandleType) -> Handle {
        Handle(handle, HandleTypeEnum::Write)
    }

    fn native(&self) -> InnerHandleType {
        self.0
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct OsPipe {
    pub span: Span,
    pub datatype: StreamDataType,

    read_handle: Handle,
    write_handle: Handle,
}

impl OsPipe {
    /// Creates a new pipe. Pipes are unidirectional streams of bytes composed of a read end and a write end. They can be used for interprocess communication.
    /// Uses `pipe(2)` on unix and `CreatePipe` on windows.
    pub fn create(span: Span) -> Result<Self, PipeError> {
        pipe_impl::create_pipe(span)
    }

    /// Closes the write end of the pipe. This is needed to signal the end of the stream to the reader.
    #[inline(always)]
    pub fn close_write(&self) -> Result<(), PipeError> {
        pipe_impl::close_handle(self.write_handle)
    }

    /// Closes the read end of the pipe. This is needed to signal we are done reading from the pipe.
    #[inline(always)]
    pub fn close_read(&self) -> Result<(), PipeError> {
        pipe_impl::close_handle(self.read_handle)
    }

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
    #[inline(always)]
    pub fn reader(&self) -> HandleReader {
        assert!(self.read_handle.1 == HandleTypeEnum::Read);
        HandleReader(self.read_handle)
    }

    /// Returns a buffered handle writer for the pipe. Prefer this over `unbuffered_writer` for better performance.
    ///
    /// ### Closing
    /// It is crucial to call `writer.close()` on the writer when you are done with it.
    /// Otherwise the buffered writer will not flush the buffer to the pipe and the reader will hang waiting for more data.
    ///
    /// If you do not want to close the handle but still want to flush the buffer, you can call `writer.flush()` instead.
    #[inline(always)]
    pub fn writer(&self) -> BufferedHandleWriter {
        assert!(self.write_handle.1 == HandleTypeEnum::Write);
        BufferedHandleWriter::new(self.write_handle)
    }

    /// Returns an unbuffered writer for the pipe. Prefer `writer` over this for better performance.
    ///
    /// ### Closing
    /// It is crucial to call `writer.close()` on the writer when you are done with it to signal the end of the stream to the reader.
    /// Failing to do so will cause the reader to hang waiting for more data.
    #[inline(always)]
    pub fn unbuffered_writer(&self) -> UnbufferedHandleWriter {
        assert!(self.write_handle.1 == HandleTypeEnum::Write);
        UnbufferedHandleWriter(self.write_handle)
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
    #[inline(always)]
    pub fn rw(&self) -> (HandleReader, BufferedHandleWriter) {
        (self.reader(), self.writer())
    }

    /// Returns a tuple containing a `HandleReader` and an `UnbufferedHandleWriter`. Prefer `rw` over this for better performance.
    #[inline(always)]
    pub fn urw(&self) -> (HandleReader, UnbufferedHandleWriter) {
        (self.reader(), self.unbuffered_writer())
    }
}

impl From<Handle> for InnerHandleType {
    fn from(val: Handle) -> Self {
        val.0
    }
}

impl From<PipeError> for std::io::Error {
    fn from(error: PipeError) -> Self {
        match error {
            PipeError::UnexpectedInvalidPipeHandle => {
                std::io::Error::new(std::io::ErrorKind::Other, "Unexpected invalid pipe handle")
            }
            PipeError::FailedToCreatePipe(error) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create pipe: {}", error.0),
            ),
            PipeError::UnsupportedPlatform => {
                std::io::Error::new(std::io::ErrorKind::Other, "Unsupported platform for pipes")
            }
            PipeError::FailedToCloseHandle(_, error) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to close pipe handle: {}", error.0),
            ),
            PipeError::FailedToRead(_, error) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read from pipe: {}", error),
            ),
            PipeError::FailedToWrite(_, error) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to write to pipe: {}", error),
            ),
            PipeError::FailedSetNamedPipeHandleState(_, error) => std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to set named pipe handle state: {:?}", error),
            ),
        }
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
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        pipe_impl::read_handle(self.0, buf).map_err(|e| e.into())
    }
}

impl UnbufferedHandleWriter {
    pub fn buffered(self) -> BufferedHandleWriter {
        BufferedHandleWriter::new(self.0)
    }
}

impl From<PipeError> for ShellError {
    fn from(error: PipeError) -> Self {
        match error {
            PipeError::UnexpectedInvalidPipeHandle => {
                ShellError::IOError("Unexpected invalid pipe handle".to_string())
            }
            PipeError::FailedToCreatePipe(error) => {
                ShellError::IOError(format!("Failed to create pipe: {}", error.0))
            }
            PipeError::UnsupportedPlatform => {
                ShellError::IOError("Unsupported platform for pipes".to_string())
            }
            PipeError::FailedToCloseHandle(v, e) => {
                ShellError::IOError(format!("Failed to close pipe handle {:?}: {}", v, e.0))
            }
            PipeError::FailedToRead(v, e) => {
                ShellError::IOError(format!("Failed to read from pipe {:?}: {}", v, e))
            }
            PipeError::FailedToWrite(v, e) => {
                ShellError::IOError(format!("Failed to write to pipe {:?}: {}", v, e))
            }
            PipeError::FailedSetNamedPipeHandleState(v, e) => ShellError::IOError(format!(
                "Failed to set named pipe handle state {:?}: {:?}",
                v, e
            )),
        }
    }
}

impl std::fmt::Display for PipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipeError::UnexpectedInvalidPipeHandle => {
                write!(f, "Unexpected invalid pipe handle")
            }
            PipeError::FailedToCreatePipe(error) => {
                write!(f, "Failed to create pipe: {}", error.0)
            }
            PipeError::UnsupportedPlatform => write!(f, "Unsupported platform for pipes"),
            PipeError::FailedToCloseHandle(v, e) => {
                write!(f, "Failed to close pipe handle {:?}: {}", v, e.0)
            }
            PipeError::FailedToRead(v, e) => {
                write!(f, "Failed to read from pipe {:?}: {}", v, e)
            }
            PipeError::FailedToWrite(v, e) => {
                write!(f, "Failed to write to pipe {:?}: {}", v, e)
            }
            PipeError::FailedSetNamedPipeHandleState(v, e) => {
                write!(f, "Failed to set named pipe handle state {:?}: {:?}", v, e)
            }
        }
    }
}

impl OsPipe {
    /// Starts a new thread that will pipe the stdout stream to the os pipe.
    ///
    /// Returns a handle to the thread if the input is a pipe and the output is an external stream.
    pub fn start_pipe(input: &mut CallInput) -> Result<Option<JoinHandle<()>>, ShellError> {
        match input {
            CallInput::Pipe(os_pipe, Some(PipelineData::ExternalStream { stdout, .. })) => {
                let handle = {
                    // unsafely move the stdout stream to the new thread by casting to a void pointer
                    let Some(stdout) = stdout.take() else {
                        return Ok(None);
                    };
                    let os_pipe = os_pipe.clone();

                    std::thread::spawn(move || {
                        let mut os_pipe = os_pipe;
                        let stdout = stdout;
                        os_pipe.datatype = stdout.datatype;
                        #[cfg(all(unix, debug_assertions))]
                        {
                            let pid = std::process::id();
                            let res_self = Command::new("ps")
                                .arg("-o")
                                .arg("comm=")
                                .arg("-p")
                                .arg(pid.to_string())
                                .output();
                            let self_name = match res_self {
                                Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
                                Err(_) => "".to_string(),
                            };
                            trace!("thread::self: {} {:?}", pid, self_name);
                            let ppid = std::os::unix::process::parent_id();
                            let res_parent = Command::new("ps")
                                .arg("-o")
                                .arg("comm=")
                                .arg("-p")
                                .arg(ppid.to_string())
                                .output();
                            let parent_name = match res_parent {
                                Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
                                Err(_) => "".to_string(),
                            };
                            trace!("thread::parent: {} {:?}", ppid, parent_name);
                            let open_fds = Command::new("lsof")
                                .arg("-p")
                                .arg(pid.to_string())
                                .output()
                                .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
                                .unwrap_or_else(|_| "".to_string());
                            trace!("thread::open fds: \n{}", open_fds);
                            // get permissions and other info for read_fd
                            let info =
                                unsafe { libc::fcntl(os_pipe.write_handle.into(), libc::F_GETFL) };
                            let acc_mode = match info & libc::O_ACCMODE {
                                libc::O_RDONLY => "read-only".to_string(),
                                libc::O_WRONLY => "write-only".to_string(),
                                libc::O_RDWR => "read-write".to_string(),
                                e => format!("unknown access mode {}", e),
                            };
                            trace!("thread::write_fd::access mode: {}", acc_mode);
                            let info =
                                unsafe { libc::fcntl(os_pipe.read_handle.into(), libc::F_GETFL) };
                            let acc_mode = match info & libc::O_ACCMODE {
                                libc::O_RDONLY => "read-only".to_string(),
                                libc::O_WRONLY => "write-only".to_string(),
                                libc::O_RDWR => "read-write".to_string(),
                                e => format!("unknown access mode {}", e),
                            };
                            trace!("thread::read_fd::access mode: {}", acc_mode);
                        }
                        trace!("OsPipe::start_pipe thread for {:?}", os_pipe);

                        let _ = os_pipe.close_read();

                        let mut writer = os_pipe.unbuffered_writer();

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
                        match writer.flush() {
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
                        let _ = os_pipe.close_write();
                        // close the pipe when the stream is finished
                    })
                };

                Ok(Some(handle))
            }
            _ => Ok(None),
        }
    }
}

#[derive(Debug)]
pub struct OSError(
    #[cfg(windows)] windows::core::Error,
    #[cfg(unix)] std::io::Error,
);

#[cfg(unix)]
impl From<std::io::Error> for OSError {
    fn from(error: std::io::Error) -> Self {
        OSError(error)
    }
}

impl std::fmt::Display for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let handle = {
            #[cfg(windows)]
            {
                self.0 .0
            }
            #[cfg(unix)]
            {
                self.0
            }
        };
        write!(f, "{:?} ({})", self.1, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipe() {
        let pipe = OsPipe::create(Span::unknown()).unwrap();
        println!("{:?}", pipe);
        let (mut reader, mut writer) = pipe.urw();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();
        pipe.close_write().unwrap();

        assert_eq!(written, 11);

        let mut buf = [0u8; 11];

        let read = reader.read(&mut buf).unwrap();
        pipe.close_read().unwrap();

        assert_eq!(read, 11);
        assert_eq!(buf, "hello world".as_bytes());
    }

    #[test]
    fn test_serialized_pipe() {
        let pipe = OsPipe::create(Span::unknown()).unwrap();
        let mut writer = pipe.unbuffered_writer();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        writer.close().unwrap();

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
        println!("{}", serialized);
        // deserialize the pipe
        let deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();
        let mut reader = deserialized.reader();

        let mut buf = [0u8; 11];

        let read = reader.read(&mut buf).unwrap();

        assert_eq!(read, 11);
        assert_eq!(buf, "hello world".as_bytes());
        reader.close().unwrap();
    }

    #[test]
    fn test_pipe_in_another_thread() {
        let pipe = OsPipe::create(Span::unknown()).unwrap();
        let mut writer = pipe.unbuffered_writer();
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
            let mut reader = deserialized.reader();

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
        let mut writer = pipe.unbuffered_writer();
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
