use nu_protocol::{Span, StreamDataType};
use serde::{Deserialize, Serialize};

use crate::{protocol::os_pipe::OSError, Handles};

use super::PipeError;

type HandleType = windows::Win32::Foundation::HANDLE;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct OsPipe {
    pub span: Span,
    pub datatype: StreamDataType,

    #[serde(with = "windows_handle_serialization")]
    read_handle: HandleType,

    #[serde(with = "windows_handle_serialization")]
    write_handle: HandleType,
}

impl OsPipe {
    pub fn create(span: Span) -> Result<Self, PipeError> {
        use windows::Win32::Security::SECURITY_ATTRIBUTES;
        use windows::Win32::System::Pipes::CreatePipe;

        let mut read_handle = windows::Win32::Foundation::INVALID_HANDLE_VALUE;
        let mut write_handle = windows::Win32::Foundation::INVALID_HANDLE_VALUE;

        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: true.into(),
        };

        unsafe { CreatePipe(&mut read_handle, &mut write_handle, Some(&attributes), 0) }
            .map_err(|e| PipeError::FailedToCreatePipe(OSError(e)))?;

        Ok(OsPipe {
            span,
            read_handle: read_handle,
            write_handle: write_handle,
            datatype: StreamDataType::Binary,
        })
    }

    pub fn close(&self, handles: Vec<Handles>) -> Result<(), PipeError> {
        use windows::Win32::Foundation::CloseHandle;

        let errors = handles
            .into_iter()
            .filter_map(|handle| {
                match &handle {
                    Handles::Read => unsafe { CloseHandle(self.read_handle) },
                    Handles::Write => unsafe { CloseHandle(self.write_handle) },
                }
                .err()
                .map(|e| (handle, OSError(e)))
            })
            .collect::<Vec<_>>();

        if !errors.is_empty() {
            return Err(PipeError::FailedToClose(errors));
        }

        Ok(())
    }
}

impl std::io::Read for OsPipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        eprintln!("OsPipe::read for {:?}", self);
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.write_handle) }?;

        let mut bytes_read = 0;
        unsafe {
            windows::Win32::Storage::FileSystem::ReadFile(
                self.read_handle,
                Some(buf),
                Some(&mut bytes_read),
                None,
            )
        }
        .map_err(|e| std::io::Error::from(e))?;

        eprintln!("OsPipe::read: {} bytes", bytes_read);

        Ok(bytes_read as usize)
    }
}

impl std::io::Write for OsPipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        eprintln!("OsPipe::write for {:?}", self);
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.read_handle) }?;

        let mut bytes_written = 0;
        unsafe {
            windows::Win32::Storage::FileSystem::WriteFile(
                self.write_handle,
                Some(buf),
                Some(&mut bytes_written),
                None,
            )
        }
        .map_err(|e| std::io::Error::from(e))?;

        eprintln!("OsPipe::write: {} bytes", bytes_written);

        Ok(bytes_written as usize)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl From<windows::core::Error> for OSError {
    fn from(error: windows::core::Error) -> Self {
        OSError(error)
    }
}

pub mod windows_handle_serialization {
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
