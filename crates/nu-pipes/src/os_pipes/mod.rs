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
}

impl PipeFd<PipeWrite> {
    /// Creates a new `OwningPipeWriter` from the given pipe file descriptor.
    pub fn into_writer(self) -> OwningPipeWriter {
        OwningPipeWriter::new(self)
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
    use crate::{
        unidirectional::{PipeRead, PipeWrite},
        FromRawPipeFd, PipeFd,
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
}
