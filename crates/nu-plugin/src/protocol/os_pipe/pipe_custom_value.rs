use std::{io::Read, process::Command};

use log::trace;
use nu_protocol::{CustomValue, ShellError, Span, Spanned, StreamDataType, Value};
use serde::{Deserialize, Serialize};

use crate::{Handle, OsPipe};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct StreamCustomValue {
    pub span: Span,
    pub os_pipe: OsPipe,
}

impl StreamCustomValue {
    pub fn new(os_pipe: OsPipe, span: Span) -> Self {
        Self { span, os_pipe }
    }
}

impl CustomValue for StreamCustomValue {
    fn clone_value(&self, span: Span) -> Value {
        Value::custom_value(Box::new(self.clone()), span)
    }

    fn value_string(&self) -> String {
        trace!(
            "{}::value_string for {:?}",
            self.typetag_name(),
            self.os_pipe
        );
        self.to_base_value(self.span)
            .map(|v| v.as_string().unwrap_or_default())
            .unwrap_or_default()
    }

    fn to_base_value(&self, span: Span) -> Result<Value, ShellError> {
        trace!(
            "{}::to_base_value for {:?}",
            self.typetag_name(),
            self.os_pipe
        );
        match self.os_pipe.datatype {
            StreamDataType::Binary => {
                let val = Vec::new();
                _ = self.os_pipe.clone().read_to_end(&mut val.clone())?;
                Ok(Value::binary(val, span))
            }
            StreamDataType::Text => {
                let mut vec = Vec::new();
                _ = self.os_pipe.clone().read_to_end(&mut vec)?;
                let string = String::from_utf8_lossy(&vec);
                Ok(Value::string(string, span))
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn span(&self) -> Span {
        self.span
    }

    #[doc(hidden)]
    fn typetag_name(&self) -> &'static str {
        match self.os_pipe.datatype {
            StreamDataType::Binary => "StreamCustomValue::Binary",
            StreamDataType::Text => "StreamCustomValue::Text",
        }
    }

    #[doc(hidden)]
    fn typetag_deserialize(&self) {
        unimplemented!("typetag_deserialize")
    }

    fn as_string(&self) -> Result<String, ShellError> {
        trace!("{}::as_string for {:?}", self.typetag_name(), self.os_pipe);

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
            trace!("plugin::self: {} {:?}", pid, self_name);
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
            trace!("plugin::parent: {} {:?}", ppid, parent_name);
            let open_fds = Command::new("lsof")
                .arg("-p")
                .arg(pid.to_string())
                .output()
                .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
                .unwrap_or_else(|_| "".to_string());
            trace!("plugin::open fds: \n{}", open_fds);
            // get permissions and other info for read_fd
            let info = unsafe { libc::fcntl(self.os_pipe.read_fd, libc::F_GETFL) };
            if info < 0 {
                trace!("plugin::fcntl failed: {}", std::io::Error::last_os_error());
            } else {
                let acc_mode = match info & libc::O_ACCMODE {
                    libc::O_RDONLY => "read-only".to_string(),
                    libc::O_WRONLY => "write-only".to_string(),
                    libc::O_RDWR => "read-write".to_string(),
                    e => format!("unknown access mode {}", e),
                };
                trace!("plugin::read_fd::access mode: {}", acc_mode);
            }
            let info = unsafe { libc::fcntl(self.os_pipe.write_fd, libc::F_GETFL) };
            if info < 0 {
                trace!("plugin::fcntl failed: {}", std::io::Error::last_os_error());
            } else {
                let acc_mode = match info & libc::O_ACCMODE {
                    libc::O_RDONLY => "read-only".to_string(),
                    libc::O_WRONLY => "write-only".to_string(),
                    libc::O_RDWR => "read-write".to_string(),
                    e => format!("unknown access mode {}", e),
                };
                trace!("plugin::write_fd::access mode: {}", acc_mode);
            }
        }
        let vec = self.read_as_string()?;
        self.os_pipe.close(Handle::Read)?;
        Ok(vec)
    }

    fn as_spanned_string(&self) -> Result<nu_protocol::Spanned<String>, ShellError> {
        Ok(Spanned {
            item: self.read_as_string()?,
            span: self.span,
        })
    }
}

impl std::io::Read for StreamCustomValue {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        trace!("StreamCustomValue::read for {:?}", self.os_pipe);
        self.os_pipe.read(buf)
    }
}

impl IntoIterator for StreamCustomValue {
    type Item = Result<Vec<u8>, ShellError>;

    type IntoIter = StreamCustomValueIterator;

    fn into_iter(self) -> Self::IntoIter {
        StreamCustomValueIterator {
            stream: self,
            done: false,
            buf: [0u8; READ_SIZE],
        }
    }
}

pub struct StreamCustomValueIterator {
    stream: StreamCustomValue,
    done: bool,
    buf: [u8; READ_SIZE],
}

const READ_SIZE: usize = 1024;

impl Iterator for StreamCustomValueIterator {
    type Item = Result<Vec<u8>, ShellError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let res = self.stream.read(&mut self.buf);
        match res {
            Ok(0) => {
                self.done = true;
                None
            }
            Ok(_) => Some(Ok(self.buf.to_vec())),
            Err(e) => Some(Err(ShellError::CantConvert {
                to_type: "binary".into(),
                from_type: self.stream.typetag_name().into(),
                span: self.stream.span(),
                help: Some(e.to_string()),
            })),
        }
    }
}

impl StreamCustomValue {
    fn read_as_string(&self) -> Result<String, ShellError> {
        let mut vec = Vec::new();
        _ = self.clone().read_to_end(&mut vec)?;
        let string = String::from_utf8_lossy(&vec);
        Ok(string.to_string())
    }
}
