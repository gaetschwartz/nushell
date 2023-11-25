use nu_protocol::{Span, StreamDataType};
use serde::{Deserialize, Serialize};

use crate::protocol::os_pipe::OSError;

use super::PipeError;

type HandleType = windows::Win32::Foundation::HANDLE;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct OsPipe {
    pub span: Span,
    pub datatype: StreamDataType,

    #[serde(with = "windows_handle_serialization")]
    read_handle: Option<HandleType>,

    #[serde(with = "windows_handle_serialization")]
    write_handle: Option<HandleType>,
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
            read_handle: Some(read_handle),
            write_handle: Some(write_handle),
            datatype: StreamDataType::Binary,
        })
    }

    pub fn close(&mut self) -> Result<(), PipeError> {
        use windows::Win32::Foundation::CloseHandle;

        let read_res = self
            .read_handle
            .map(|handle| unsafe { CloseHandle(handle) });
        let write_res = self
            .write_handle
            .map(|handle| unsafe { CloseHandle(handle) });

        if let Some(e) = vec![read_res, write_res]
            .iter()
            .find_map(|res| res.as_ref().map(|e| e.as_ref().err()).flatten())
        {
            return Err(PipeError::FailedToClose(Some(OSError(e.clone()))));
        }

        Ok(())
    }
}

impl std::io::Read for OsPipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut bytes_read = 0;

        let Some(read_handle) = self.read_handle else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Missing read handle",
            ));
        };

        unsafe {
            windows::Win32::Storage::FileSystem::ReadFile(
                read_handle,
                Some(buf),
                Some(&mut bytes_read),
                None,
            )
        }
        .map_err(|e| std::io::Error::from(e))?;

        Ok(bytes_read as usize)
    }
}

impl std::io::Write for OsPipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut bytes_written = 0;

        let Some(write_handle) = self.write_handle else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Missing write handle",
            ));
        };

        unsafe {
            windows::Win32::Storage::FileSystem::WriteFile(
                write_handle,
                Some(buf),
                Some(&mut bytes_written),
                None,
            )
        }
        .map_err(|e| std::io::Error::from(e))?;

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

    #[derive(Debug, Deserialize, Serialize)]
    struct WrappedHandle(
        #[serde(
            deserialize_with = "deserialize_handle",
            serialize_with = "serialize_handle"
        )]
        windows::Win32::Foundation::HANDLE,
    );

    pub fn deserialize_handle<'de, D>(
        deserializer: D,
    ) -> Result<windows::Win32::Foundation::HANDLE, D::Error>
    where
        D: Deserializer<'de>,
    {
        let handle = <isize>::deserialize(deserializer)?;
        Ok(windows::Win32::Foundation::HANDLE(handle))
    }

    pub fn serialize_handle<S>(
        handle: &windows::Win32::Foundation::HANDLE,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        handle.0.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Option<windows::Win32::Foundation::HANDLE>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<WrappedHandle>::deserialize(deserializer).map(
            |opt_wrapped: Option<WrappedHandle>| {
                opt_wrapped.map(|wrapped: WrappedHandle| wrapped.0)
            },
        )
    }

    pub fn serialize<S>(
        handle: &Option<windows::Win32::Foundation::HANDLE>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let opt_wrapped = handle
            .as_ref()
            .map(|handle: &windows::Win32::Foundation::HANDLE| WrappedHandle(handle.clone()));
        opt_wrapped.serialize(serializer)
    }
}
