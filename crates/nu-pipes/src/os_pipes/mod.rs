#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};
use std::{marker::PhantomData, ops::Deref};

use serde::{Deserialize, Serialize};

use crate::{
    errors::{PipeError, PipeResult},
    PipeReader,
};

use self::{
    io::{CloseOwningError, OwningPipeReader, OwningPipeWriter, PipeWriter},
    sys::NativeFd,
    unidirectional::{PipeFdType, PipeFdTypeEnum, PipeRead, PipeWrite},
};
/// The inner type of a pipe file descriptor, i32 on Unix and HANDLE on Windows.
pub type RawPipeFd = i32;

pub mod io;
pub mod unidirectional;

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(unix, path = "unix.rs")]
mod sys;

/// The capacity of pipe buffers.
pub const PIPE_BUFFER_CAPACITY: usize = 1024 * 8;

pub(crate) trait PipeImplBase {
    fn create_pipe() -> Result<OsPipe, PipeError>;

    fn read(fd: impl AsPipeFd<PipeRead>, buf: &mut [u8]) -> PipeResult<usize>;

    fn write(fd: impl AsPipeFd<PipeWrite>, buf: &[u8]) -> PipeResult<usize>;

    fn close_pipe<T: PipeFdType>(fd: impl AsPipeFd<T>) -> PipeResult<()>;

    fn dup<T: PipeFdType>(fd: impl AsPipeFd<T>) -> PipeResult<PipeFd<T>>;

    const INVALID_FD_VALUE: NativeFd;
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub(crate) struct OsPipe {
    read_fd: PipeFd<PipeRead>,
    write_fd: PipeFd<PipeWrite>,
}

/// A pipe file descriptor.
#[repr(transparent)]
pub struct PipeFd<T: PipeFdType>(pub(crate) NativeFd, pub(crate) PhantomData<T>);

impl<T: PipeFdType> PipeFd<T> {
    /// Duplicates the current pipe file descriptor making it inheritable.
    pub fn into_inheritable(self) -> Result<PipeFd<T>, PipeError> {
        let dup = sys::PipeImpl::dup(&self)?;
        self.close()?;
        Ok(dup)
    }
}

impl PipeFd<PipeRead> {
    /// Creates a new `OwningPipeReader` from the given pipe file descriptor.
    pub fn into_reader(self) -> OwningPipeReader {
        OwningPipeReader::new(self)
    }

    /// The pipe fd of stdin.
    pub fn stdin() -> PipeFd<PipeRead> {
        unsafe { PipeFd::from_raw_pipe_fd(0) }
    }
}

impl PipeFd<PipeWrite> {
    /// Creates a new `OwningPipeWriter` from the given pipe file descriptor.
    pub fn into_writer(self) -> OwningPipeWriter {
        OwningPipeWriter::new(self)
    }

    /// The pipe fd of stdout.
    pub fn stdout() -> PipeFd<PipeWrite> {
        unsafe { PipeFd::from_raw_pipe_fd(1) }
    }

    /// The pipe fd of stderr.
    pub fn stderr() -> PipeFd<PipeWrite> {
        unsafe { PipeFd::from_raw_pipe_fd(2) }
    }
}

impl<T: PipeFdType> PipeFd<T> {
    /// Duplicates the current pipe file descriptor.
    pub fn try_clone(&self) -> Result<PipeFd<T>, PipeError> {
        sys::PipeImpl::dup(self)
    }
}

impl<T: PipeFdType> std::fmt::Debug for PipeFd<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(windows)]
        let fd = self.0 .0;
        #[cfg(unix)]
        let fd = self.0;
        match T::TYPE {
            PipeFdTypeEnum::Read => write!(f, "PipeFd::Read({})", fd),
            PipeFdTypeEnum::Write => write!(f, "PipeFd::Write({})", fd),
            PipeFdTypeEnum::Unknown => write!(f, "PipeFd::Unknown({})", fd),
        }
    }
}

impl<T: PipeFdType> PipeFd<T> {
    /// Closes the pipe file descriptor.
    pub fn close(self) -> Result<(), CloseOwningError<PipeFd<T>, PipeError>> {
        match sys::PipeImpl::close_pipe(&self) {
            Ok(()) => Ok(()),
            Err(e) => Err(CloseOwningError::new(e, self)),
        }
    }
}

impl<T: PipeFdType> From<PipeFd<T>> for NativeFd {
    fn from(val: PipeFd<T>) -> Self {
        val.0
    }
}

trait IntoPipeFd<T: PipeFdType>: AsNativeFd {
    unsafe fn into_pipe_fd(self) -> PipeFd<T>;
}
/// A Trait for types that can be converted into a pipe file descriptor.
pub trait FromRawPipeFd {
    /// Creates a new `PipeFd` from the given raw file descriptor.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it cannot guarantee that the given file descriptor
    /// is a valid pipe file descriptor ( it could be closed already, for example)
    unsafe fn from_raw_pipe_fd(fd: RawPipeFd) -> Self;
}

/// A Trait for types that can be converted from a NativeFd.
pub trait FromNativeFd: Sized {
    /// Creates a new `PipeFd` from the given native handle.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it cannot guarantee that the given handle
    /// is a valid pipe handle ( it could be closed already, for example)
    unsafe fn from_native_fd(fd: NativeFd) -> Self;
}

/// A trait for types that can be converted as a raw pipe file descriptor.
pub trait AsRawPipeFd {
    /// Returns the raw file descriptor of the object.
    ///
    /// # Safety
    ///
    /// The returned file descriptor is not guaranteed to be valid and this could be used to violate the IO safety
    unsafe fn as_raw_pipe_fd(&self) -> RawPipeFd;
}

/// A trait for types that can be converted as a native fd.
pub trait AsNativeFd {
    /// Returns the native handle of the object.
    ///
    /// # Safety
    ///
    /// The returned handle is not guaranteed to be valid and this could be used to violate the IO safety
    /// provided by the library.
    unsafe fn native_fd(&self) -> NativeFd;
}
trait NativeFdEq: AsNativeFd {
    fn eq(&self, other: impl AsNativeFd) -> bool;
}

impl<T: NativeFdEq> NativeFdEq for &T {
    #[inline]
    fn eq(&self, other: impl AsNativeFd) -> bool {
        unsafe { self.native_fd() == other.native_fd() }
    }
}

impl<T: PipeFdType> AsNativeFd for PipeFd<T> {
    #[inline]
    unsafe fn native_fd(&self) -> NativeFd {
        self.0
    }
}
impl AsNativeFd for NativeFd {
    #[inline]
    unsafe fn native_fd(&self) -> NativeFd {
        *self
    }
}
impl<T: AsNativeFd> AsNativeFd for &T {
    #[inline]
    unsafe fn native_fd(&self) -> NativeFd {
        (*self).native_fd()
    }
}

#[cfg(unix)]
impl<T: PipeFdType> AsRawFd for PipeFd<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.0 as _
    }
}

#[cfg(unix)]
impl<T: PipeFdType> AsFd for PipeFd<T> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.as_raw_fd()) }
    }
}

/// A trait for types that can be converted as a pipe file descriptor.
pub trait AsPipeFd<T: PipeFdType> {
    /// Returns the pipe file descriptor of the object.
    fn as_pipe_fd(&self) -> &PipeFd<T>;
}
impl<T: PipeFdType> AsPipeFd<T> for PipeFd<T> {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<T> {
        self
    }
}
impl AsPipeFd<PipeRead> for PipeReader<'_> {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<PipeRead> {
        self.fd
    }
}
impl AsPipeFd<PipeWrite> for PipeWriter<'_> {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<PipeWrite> {
        self.fd
    }
}
impl AsPipeFd<PipeRead> for OsPipe {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<PipeRead> {
        &self.read_fd
    }
}
impl AsPipeFd<PipeWrite> for OsPipe {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<PipeWrite> {
        &self.write_fd
    }
}

impl<T: PipeFdType, F: AsPipeFd<T>> AsPipeFd<T> for &F {
    #[inline]
    fn as_pipe_fd(&self) -> &PipeFd<T> {
        (*self).as_pipe_fd()
    }
}

impl<T: PipeFdType> std::fmt::Display for PipeFd<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fd = unsafe { self.as_raw_pipe_fd() };
        write!(f, "{:?} ({})", T::NAME, fd)
    }
}

impl<T: PipeFdType> Deref for PipeFd<T> {
    type Target = NativeFd;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
struct PipeFdSer(#[cfg_attr(windows, serde(with = "sys::FdSerializable"))] pub(crate) NativeFd);

impl<T: PipeFdType> Serialize for PipeFd<T> {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> Result<<S as serde::Serializer>::Ok, <S as serde::Serializer>::Error>
    where
        S: serde::Serializer,
    {
        (PipeFdSer(self.0), T::NAME).serialize(serializer)
    }
}

impl<'de, T: PipeFdType> Deserialize<'de> for PipeFd<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (fd, name) = <(PipeFdSer, char)>::deserialize(deserializer)?;
        if name != T::NAME {
            return Err(serde::de::Error::custom(format!(
                "expected pipe type {}, got {}",
                T::NAME,
                name
            )));
        }
        Ok(PipeFd(fd.0, PhantomData))
    }
}

impl<T: PipeFdType, U: PipeFdType> PartialEq<PipeFd<U>> for PipeFd<T> {
    fn eq(&self, other: &PipeFd<U>) -> bool {
        self.0 == other.0 && T::TYPE == U::TYPE
    }
}
impl<T: PipeFdType> Eq for PipeFd<T> {}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use crate::{
        unidirectional::{pipe, PipeRead, PipeWrite},
        FromRawPipeFd, PipeFd,
    };

    use std::{
        io::{Read, Write},
        process::Command,
    };

    #[test]
    fn pipe_fd_eq_if_same_native_fd() {
        let fd1 = unsafe { PipeFd::<PipeRead>::from_raw_pipe_fd(1) };
        let fd2 = unsafe { PipeFd::<PipeRead>::from_raw_pipe_fd(1) };
        assert_eq!(fd1, fd2);

        let fd3 = unsafe { PipeFd::<PipeRead>::from_raw_pipe_fd(2) };
        assert_ne!(fd1, fd3);
    }

    #[test]
    fn pipe_fd_neq_if_diff_native_fd() {
        let fd1 = unsafe { PipeFd::<PipeRead>::from_raw_pipe_fd(1) };
        let fd2 = unsafe { PipeFd::<PipeRead>::from_raw_pipe_fd(2) };
        assert_ne!(fd1, fd2);
    }

    #[test]
    fn pipe_fd_neq_if_diff_type() {
        let fd1 = unsafe { PipeFd::<PipeRead>::from_raw_pipe_fd(1) };
        let fd2 = unsafe { PipeFd::<PipeWrite>::from_raw_pipe_fd(1) };
        assert_ne!(fd1, fd2);
    }

    trait ReadAsString {
        fn read_as_string(&mut self) -> Result<String, std::io::Error>;
    }

    impl<R: Read> ReadAsString for R {
        fn read_as_string(&mut self) -> Result<String, std::io::Error> {
            let mut buf = String::new();
            self.read_to_string(&mut buf).map(|_| buf)
        }
    }

    fn as_string(r: Option<impl Read>) -> String {
        r.map(|mut s| s.read_as_string().unwrap())
            .unwrap_or("None".to_string())
    }

    pub(crate) fn named_thread<
        T: 'static + Send,
        F: 'static + Send + FnOnce() -> T,
        S: Into<String>,
    >(
        name: S,
        f: F,
    ) -> Result<std::thread::JoinHandle<T>, std::io::Error> {
        std::thread::Builder::new().name(name.into()).spawn(f)
    }

    // This test among others are ran in serial to ensure that the pipe file descriptors
    // are not reused between tests. This is because the pipe file descriptors are
    // global and we don't want to close a pipe file descriptor that is still in use
    // by another test.
    // We use the `serial_test` crate to ensure that the tests are ran in serial.
    #[test]
    #[serial(nu_pipes)]
    fn pipes_readwrite() {
        let (read, write) = pipe().unwrap();
        let mut reader = read.into_reader();
        let mut writer = write.into_writer();
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
    #[serial(nu_pipes)]

    fn pipes_with_closed_read_end_cant_write() {
        let (read, write) = pipe().unwrap();
        let mut reader = read.into_reader();
        let mut writer = write.into_writer();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();
        writer.flush().unwrap();

        assert_eq!(written, 11);

        let mut buf = [0u8; 11];
        let read = reader.read(&mut buf).unwrap();
        assert_eq!(read, 11);
        assert_eq!(&buf[..], "hello world".as_bytes());

        reader.close().unwrap();

        let written = writer.write("woohoo ohoho".as_bytes());
        let flushed = writer.flush();

        assert!(
            written.is_err() || flushed.is_err(),
            "Expected error, but {}",
            written
                .map(|b| format!("wrote {} bytes", b))
                .or_else(|_| flushed.map(|_| "flushed".to_string()))
                .unwrap()
        );
    }

    #[test]
    #[serial(nu_pipes)]

    fn pipe_read_write_in_thread() {
        let (read, write) = pipe().unwrap();
        let mut writer = write.into_writer();
        // write hello world to the pipe
        let written = writer.write("hello world".as_bytes()).unwrap();

        assert_eq!(written, 11);
        writer.close().unwrap();

        // serialize the pipe
        let serialized = serde_json::to_string(&read).unwrap();
        // spawn a new process
        let (read, buf) = named_thread("thread@pipe_in_another_thread", move || {
            // deserialize the pipe
            let deserialized: PipeFd<PipeRead> = serde_json::from_str(&serialized).unwrap();
            let mut reader = deserialized.into_reader();

            let mut buf = [0u8; 32];

            let read = reader.read(&mut buf).unwrap();

            reader.close().unwrap();

            (read, buf)
        })
        .unwrap()
        .join()
        .unwrap();

        assert_eq!(read, 11);
        assert_eq!(&buf[..read], "hello world".as_bytes());
    }

    trait ReadExact: Read {
        fn read_exactly_n<const N: usize>(&mut self) -> Result<[u8; N], std::io::Error> {
            let mut buf = [0u8; N];
            self.read_exact(&mut buf)?;
            Ok(buf)
        }
    }
    impl<R: Read> ReadExact for R {}

    #[test]
    #[serial(nu_pipes)]

    fn pipe_in_another_thread_cancelled() {
        let (read, write) = pipe().unwrap();

        let thread: std::thread::JoinHandle<Result<(), std::io::Error>> =
            named_thread("thread@pipe_in_another_thread_cancelled", move || {
                let mut writer = write.into_writer();

                // serialize the pipe
                loop {
                    eprintln!("Writing to pipe...");
                    _ = writer.write("hello world".as_bytes())?;
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    writer.flush()?;
                }
            })
            .unwrap();

        let mut reader = read.into_reader();
        eprintln!("Starting to read from pipe...");
        let s1 = reader.read_exactly_n::<11>().unwrap();
        eprintln!("Read from pipe... (1)");
        assert_eq!(&s1[..], b"hello world");
        eprintln!("Read from pipe... (2)");
        let s2 = reader.read_exactly_n::<11>().unwrap();
        assert_eq!(&s2[..], b"hello world");
        eprintln!("Closing pipe...");
        reader.close().unwrap();
        eprintln!("Joining thread...");
        let joined = thread.join().unwrap();
        println!("This error is expected: {:?}", joined);
        match joined {
            Ok(_) => panic!("Thread should have been cancelled"),
            Err(e) => match e.kind() {
                std::io::ErrorKind::BrokenPipe => {}
                _ => panic!("Unexpected error: {:?}", e),
            },
        }
    }

    #[test]
    #[serial(nu_pipes)]

    fn test_pipe_in_another_process() {
        println!("Compiling pipe_echoer...");
        const BINARY_NAME: &str = "pipe_echoer";

        Command::new("cargo")
            .arg("build")
            .arg("-q")
            .arg("--bin")
            .arg(BINARY_NAME)
            .spawn()
            .unwrap()
            .wait()
            .unwrap();

        let (read, write) = pipe().unwrap();
        println!("read: {:?}", read);
        println!("write: {:?}", write);
        let read_dup = read.try_clone().unwrap();

        // serialize the pipe
        let json = serde_json::to_string(&read_dup).unwrap();
        read.close().unwrap();

        println!("Running pipe_echoer...");

        // spawn a new process
        let mut res = Command::new("cargo")
            .arg("run")
            .arg("--quiet")
            .arg("--bin")
            .arg(BINARY_NAME)
            .arg(json)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();

        // write hello world to the pipe
        let mut writer = write.into_writer();
        let written = writer.write(b"hello world").unwrap();
        assert_eq!(written, 11);
        writer.flush().unwrap();
        writer.close().unwrap();

        println!("Waiting for pipe_echoer to finish...");

        let code = res.wait().unwrap();
        // read_dup.close().unwrap();

        if !code.success() {
            panic!("Process failed: {:?}", code);
        }

        assert_eq!(as_string(res.stdout.take()), "hello world\n");
    }
}
