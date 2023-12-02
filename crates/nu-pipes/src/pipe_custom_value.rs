use nu_protocol::{CustomValue, ShellError, Span, Spanned, StreamDataType, Value};
use serde::{Deserialize, Serialize};
use std::io::Read;

use crate::unidirectional::{PipeRead, UnOpenedPipe};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct StreamCustomValue {
    pub span: Span,
    pub os_pipe: UnOpenedPipe<PipeRead>,
}

impl StreamCustomValue {
    pub fn new(os_pipe: UnOpenedPipe<PipeRead>, span: Span) -> Self {
        Self { span, os_pipe }
    }
}

impl CustomValue for StreamCustomValue {
    fn clone_value(&self, span: Span) -> Value {
        Value::custom_value(Box::new(self.clone()), span)
    }

    fn value_string(&self) -> String {
        self.to_base_value(self.span)
            .map(|v| v.as_string().unwrap_or_default())
            .unwrap_or_default()
    }

    fn to_base_value(&self, span: Span) -> Result<Value, ShellError> {
        let mut reader = self.os_pipe.open()?;
        let mut vec = Vec::new();
        _ = reader.read_to_end(&mut vec)?;

        match self.os_pipe.datatype {
            StreamDataType::Binary => Ok(Value::binary(vec, span)),
            StreamDataType::Text => Ok(Value::string(String::from_utf8_lossy(&vec), span)),
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
        let mut reader = self.os_pipe.open()?;
        let mut vec = Vec::new();
        _ = reader.read_to_end(&mut vec)?;
        let string = String::from_utf8_lossy(&vec);

        reader.close()?;
        Ok(string.to_string())
    }

    fn as_spanned_string(&self) -> Result<nu_protocol::Spanned<String>, ShellError> {
        Ok(Spanned {
            item: self.as_string()?,
            span: self.span,
        })
    }
}
