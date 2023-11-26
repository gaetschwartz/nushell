use std::{
    io::{Read, Write},
    process::Command,
    thread::JoinHandle,
};

use nu_protocol::{PipelineData, ShellError, Span};
pub use pipe_custom_value::StreamCustomValue;
use serde::{Deserialize, Serialize};

pub use self::pipe_impl::OsPipe;

trait OsPipeTrait: Read + Write + Send + Sync + Serialize + Deserialize<'static> {
    fn create(span: Span) -> Result<Self, PipeError>;
    fn close(&mut self) -> Result<(), PipeError>;
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
    FailedToClose(Vec<(Handles, OSError)>),
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
            PipeError::FailedToClose(v) => {
                let mut s = String::from("Failed to close pipe");
                if v.len() > 1 {
                    s.push('s');
                }
                s.push_str(": ");
                s.push_str(
                    &v.iter()
                        .map(|(h, e)| format!("{} ({})", h, e.0))
                        .collect::<Vec<String>>()
                        .join(", "),
                );
                ShellError::IOError(s)
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
                    let stdout = stdout.take();
                    let Some(stdout) = stdout else {
                        return Ok(None);
                    };
                    let os_pipe = os_pipe.clone();

                    std::thread::spawn(move || {
                        let mut os_pipe = os_pipe;
                        let stdout = stdout;
                        os_pipe.datatype = stdout.datatype;
                        #[cfg(unix)]
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
                            eprintln!("thread::self: {} {:?}", pid, self_name);
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
                            eprintln!("thread::parent: {} {:?}", ppid, parent_name);
                            let open_fds = Command::new("lsof")
                                .arg("-p")
                                .arg(pid.to_string())
                                .output()
                                .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
                                .unwrap_or_else(|_| "".to_string());
                            eprintln!("thread::open fds: \n{}", open_fds);
                            // get permissions and other info for read_fd
                            let info = unsafe { libc::fcntl(os_pipe.write_fd, libc::F_GETFL) };
                            let acc_mode = match info & libc::O_ACCMODE {
                                libc::O_RDONLY => "read-only".to_string(),
                                libc::O_WRONLY => "write-only".to_string(),
                                libc::O_RDWR => "read-write".to_string(),
                                e => format!("unknown access mode {}", e),
                            };
                            eprintln!("thread::write_fd::access mode: {}", acc_mode);
                            let info = unsafe { libc::fcntl(os_pipe.read_fd, libc::F_GETFL) };
                            let acc_mode = match info & libc::O_ACCMODE {
                                libc::O_RDONLY => "read-only".to_string(),
                                libc::O_WRONLY => "write-only".to_string(),
                                libc::O_RDWR => "read-write".to_string(),
                                e => format!("unknown access mode {}", e),
                            };
                            eprintln!("thread::read_fd::access mode: {}", acc_mode);
                        }
                        eprintln!("OsPipe::start_pipe thread for {:?}", os_pipe);

                        stdout.stream.for_each(|e| match e {
                            Ok(ref e) => {
                                let written = os_pipe.write(e.as_slice());
                                match written {
                                    Ok(written) => {
                                        if written != e.len() {
                                            eprintln!(
                                                "OsPipe::start_pipe thread partial write to pipe: \
                                             {} bytes written",
                                                written
                                            );
                                        } else {
                                            eprintln!(
                                                "OsPipe::start_pipe thread wrote {} bytes to pipe",
                                                written
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "OsPipe::start_pipe thread error: failed to write to \
                                         pipe: {:?}",
                                            e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("OsPipe::start_pipe thread error: {:?}", e);
                            }
                        });
                        eprintln!("OsPipe::start_pipe thread finished writing to pipe");
                        let _ = os_pipe.close(Handles::write());
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

#[derive(Debug, PartialEq, Eq, Clone)]

pub enum Handles {
    Read,
    Write,
}

impl Handles {
    pub fn all() -> Vec<Handles> {
        vec![Handles::Read, Handles::Write]
    }

    pub fn read() -> Vec<Handles> {
        vec![Handles::Read]
    }

    pub fn write() -> Vec<Handles> {
        vec![Handles::Write]
    }
}

impl From<Handles> for Vec<Handles> {
    fn from(val: Handles) -> Self {
        vec![val]
    }
}

impl std::fmt::Display for Handles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Handles::Read => write!(f, "read"),
            Handles::Write => write!(f, "write"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn test_pipe() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        // write hello world to the pipe
        let written = pipe.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        let mut buf = [0u8; 11];

        let read = pipe.read(&mut buf).unwrap();

        assert_eq!(read, 11);
        assert_eq!(buf, "hello world".as_bytes());
    }

    #[test]
    fn test_serialized_pipe() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        // write hello world to the pipe
        let written = pipe.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
        println!("{}", serialized);
        // deserialize the pipe
        let mut deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();

        let mut buf = [0u8; 11];

        let read = deserialized.read(&mut buf).unwrap();

        assert_eq!(read, 11);
        assert_eq!(buf, "hello world".as_bytes());
    }

    #[test]
    fn test_pipe_in_another_thread() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        // write hello world to the pipe
        let written = pipe.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
        // spawn a new process
        std::thread::spawn(move || {
            // deserialize the pipe
            let mut deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();

            let mut buf = [0u8; 11];

            let read = deserialized.read(&mut buf).unwrap();

            assert_eq!(read, 11);
            assert_eq!(buf, "hello world".as_bytes());
        });
    }

    #[test]
    fn test_pipe_in_another_process() {
        let mut pipe = OsPipe::create(Span::unknown()).unwrap();
        // write hello world to the pipe
        let written = pipe.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);

        // serialize the pipe
        let serialized = serde_json::to_string(&pipe).unwrap();
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
            eprintln!("stderr: {}", String::from_utf8_lossy(res.stderr.as_slice()));
            assert!(false);
        }

        assert_eq!(res.status.success(), true);
        assert_eq!(
            String::from_utf8_lossy(res.stderr.as_slice()),
            "hello world\n"
        );
    }
}
