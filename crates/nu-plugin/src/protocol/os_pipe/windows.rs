use std::{io, ptr};

// use log::trace;
use nu_protocol::{Span, StreamDataType};

use crate::{protocol::os_pipe::OSError, Handle, OsPipe};
use windows::Win32::{
    Foundation::{CloseHandle, BOOL, HANDLE, INVALID_HANDLE_VALUE},
    Security::SECURITY_ATTRIBUTES,
    System::Pipes::{CreatePipe, SetNamedPipeHandleState, TransactNamedPipe, PIPE_TYPE_MESSAGE},
};

use super::{PipeError, PipeResult};

const SECURITY_ATTRIBUTES_SIZE: u32 = std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32;

const DEFAULT_SECURITY_ATTRIBUTES: SECURITY_ATTRIBUTES = SECURITY_ATTRIBUTES {
    nLength: SECURITY_ATTRIBUTES_SIZE,
    lpSecurityDescriptor: ptr::null_mut(),
    // bInheritHandle: BOOL::from(true),
    bInheritHandle: BOOL(1),
};

pub fn create_pipe(span: Span) -> Result<OsPipe, PipeError> {
    let mut read_handle = INVALID_HANDLE_VALUE;
    let mut write_handle = INVALID_HANDLE_VALUE;

    unsafe {
        CreatePipe(
            &mut read_handle,
            &mut write_handle,
            Some(&DEFAULT_SECURITY_ATTRIBUTES),
            0,
        )
    }
    .map_err(|e| PipeError::FailedToCreatePipe(OSError(e)))?;
    let read_handle = Handle::Read(read_handle);
    let write_handle = Handle::Write(write_handle);

    unsafe { SetNamedPipeHandleState(read_handle.native(), Some(&PIPE_TYPE_MESSAGE), None, None) }
        .map_err(|e| PipeError::FailedSetNamedPipeHandleState(read_handle, e.into()))?;

    Ok(OsPipe {
        span,
        read_handle,
        write_handle,
        datatype: StreamDataType::Binary,
    })
}

// fn header() -> String {
//     format!(
//         "[{}#{}:{:?}]",
//         PathBuf::from(std::env::args().next().unwrap_or_default())
//             .file_name()
//             .unwrap_or_default()
//             .to_string_lossy(),
//         std::process::id(),
//         std::thread::current().id()
//     )
// }

pub fn close_handle(handle: Handle) -> PipeResult<()> {
    // trace!("{} OsPipe::close for {:?}", header(), handle,);
    unsafe { CloseHandle(HANDLE::from(handle)) }
        .map_err(|e| PipeError::FailedToCloseHandle(handle, OSError(e)))
}

pub(crate) fn read_handle(handle: Handle, buf: &mut [u8]) -> PipeResult<usize> {
    // trace!("{} OsPipe::read for {:?}", header(), handle,);

    let mut bytes_read = 0;
    unsafe {
        TransactNamedPipe(
            handle.native(),
            None,
            0,
            Some(buf.as_mut_ptr() as *mut _),
            buf.len() as u32,
            &mut bytes_read,
            None,
        )
    }
    .map_err(|e| PipeError::FailedToRead(handle, e.into()))?;

    // trace!("{} OsPipe::read: {} bytes", header(), bytes_read);

    Ok(bytes_read as usize)
}

pub(crate) fn write_handle(handle: Handle, buf: &[u8]) -> io::Result<usize> {
    // trace!(
    //     "{} OsPipe::write for {:?} ({} bytes)",
    //     header(),
    //     handle,
    //     buf.len()
    // );

    let mut bytes_written = 0;
    unsafe {
        TransactNamedPipe(
            handle.native(),
            Some(buf.as_ptr() as *mut _),
            buf.len() as u32,
            None,
            0,
            &mut bytes_written,
            None,
        )
    }
    .map_err(|e| PipeError::FailedToWrite(handle, e.into()))?;

    // trace!("OsPipe::write: {} bytes", bytes_written);

    Ok(bytes_written as usize)
}

impl From<windows::core::Error> for OSError {
    fn from(error: windows::core::Error) -> Self {
        OSError(error)
    }
}

pub mod handle_serialization {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<windows::Win32::Foundation::HANDLE, D::Error>
    where
        D: Deserializer<'de>,
    {
        let handle = <isize>::deserialize(deserializer)?;
        Ok(windows::Win32::Foundation::HANDLE(handle))
    }

    pub fn serialize<S>(
        handle: &windows::Win32::Foundation::HANDLE,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        handle.0.serialize(serializer)
    }
}
