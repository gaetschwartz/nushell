use std::{io::Read, process::Command};

use nu_protocol::{CustomValue, ShellError, Span, Spanned, StreamDataType, Value};
use serde::{Deserialize, Serialize};

use crate::{Handles, OsPipe};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct StreamCustomValue {
    pub span: Span,
    pub os_pipe: OsPipe,
    vec: Option<Vec<u8>>,
}

impl StreamCustomValue {
    pub fn new(os_pipe: OsPipe, span: Span) -> Self {
        Self {
            span,
            os_pipe,
            vec: None,
        }
    }

    pub fn read_pipe_to_end(&mut self) -> Result<&Vec<u8>, ShellError> {
        eprintln!(
            "{}::read_pipe_to_end for {:?}",
            self.typetag_name(),
            self.os_pipe
        );
        if self.vec.is_none() {
            let mut vec = Vec::new();
            _ = self.os_pipe.clone().read_to_end(&mut vec)?;
            self.vec = Some(vec);
        }
        if let Some(vec) = &self.vec {
            Ok(vec)
        } else {
            unreachable!()
        }
    }
}

impl CustomValue for StreamCustomValue {
    fn clone_value(&self, span: Span) -> Value {
        Value::custom_value(Box::new(self.clone()), span)
    }

    fn value_string(&self) -> String {
        eprintln!(
            "{}::value_string for {:?}",
            self.typetag_name(),
            self.os_pipe
        );
        self.to_base_value(self.span)
            .map(|v| v.as_string().unwrap_or_default())
            .unwrap_or_default()
    }

    fn to_base_value(&self, span: Span) -> Result<Value, ShellError> {
        eprintln!(
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

    fn as_binary(&self) -> Result<&[u8], ShellError> {
        let vec = self.vec.as_ref().ok_or_else(|| ShellError::CantConvert {
            to_type: "binary".into(),
            from_type: self.typetag_name().into(),
            span: self.span(),
            help: None,
        })?;
        Ok(vec.as_slice())
    }

    fn as_string(&self) -> Result<String, ShellError> {
        eprintln!("{}::as_string for {:?}", self.typetag_name(), self.os_pipe);

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
            eprintln!("plugin::self: {} {:?}", pid, self_name);
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
            eprintln!("plugin::parent: {} {:?}", ppid, parent_name);
            let open_fds = Command::new("lsof")
                .arg("-p")
                .arg(pid.to_string())
                .output()
                .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
                .unwrap_or_else(|_| "".to_string());
            eprintln!("plugin::open fds: \n{}", open_fds);
            // get permissions and other info for read_fd
            let info = unsafe { libc::fcntl(self.os_pipe.read_fd, libc::F_GETFL) };
            if info < 0 {
                eprintln!("plugin::fcntl failed: {}", std::io::Error::last_os_error());
            } else {
                let acc_mode = match info & libc::O_ACCMODE {
                    libc::O_RDONLY => "read-only".to_string(),
                    libc::O_WRONLY => "write-only".to_string(),
                    libc::O_RDWR => "read-write".to_string(),
                    e => format!("unknown access mode {}", e),
                };
                eprintln!("plugin::read_fd::access mode: {}", acc_mode);
            }
            let info = unsafe { libc::fcntl(self.os_pipe.write_fd, libc::F_GETFL) };
            if info < 0 {
                eprintln!("plugin::fcntl failed: {}", std::io::Error::last_os_error());
            } else {
                let acc_mode = match info & libc::O_ACCMODE {
                    libc::O_RDONLY => "read-only".to_string(),
                    libc::O_WRONLY => "write-only".to_string(),
                    libc::O_RDWR => "read-write".to_string(),
                    e => format!("unknown access mode {}", e),
                };
                eprintln!("plugin::write_fd::access mode: {}", acc_mode);
            }
        }
        let mut vec = Vec::new();
        _ = self.os_pipe.clone().read_to_end(&mut vec)?;
        self.os_pipe.close(Handles::read())?;
        Ok(String::from_utf8_lossy(&vec).to_string())
    }

    fn as_spanned_string(&self) -> Result<nu_protocol::Spanned<String>, ShellError> {
        self.as_binary()
            .map(|b| String::from_utf8_lossy(b).to_string())
            .map(|s| Spanned {
                item: s,
                span: self.span,
            })
    }
}
