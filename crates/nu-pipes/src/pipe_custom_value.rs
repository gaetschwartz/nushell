use nu_protocol::{CustomValue, ShellError, Span, Spanned, StreamDataType, Value};
use serde::{Deserialize, Serialize};
use std::{io::Read, sync::OnceLock};

use crate::unidirectional::{PipeRead, UnOpenedPipe};

#[derive(Serialize, Deserialize, Debug)]
pub struct StreamCustomValue {
    pub span: Span,
    pub os_pipe: UnOpenedPipe<PipeRead>,
    #[serde(skip, default)]
    pub cell: OnceLock<Vec<u8>>,
}

impl StreamCustomValue {
    pub fn new(os_pipe: UnOpenedPipe<PipeRead>, span: Span) -> Self {
        Self {
            span,
            os_pipe,
            cell: OnceLock::new(),
        }
    }
}

impl CustomValue for StreamCustomValue {
    fn clone_value(&self, span: Span) -> Value {
        Value::custom_value(Box::new(Self::new(self.os_pipe.clone(), span)), span)
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
        let vec = self.as_binary()?;
        let string = String::from_utf8_lossy(vec);

        Ok(string.to_string())
    }

    fn as_spanned_string(&self) -> Result<nu_protocol::Spanned<String>, ShellError> {
        Ok(Spanned {
            item: self.as_string()?,
            span: self.span,
        })
    }

    fn as_binary(&self) -> Result<&[u8], ShellError> {
        if let Some(cell) = self.cell.get() {
            return Ok(cell.as_slice());
        }

        let mut reader = self.os_pipe.open()?;
        let mut vec = Vec::new();

        _ = reader.read_to_end(&mut vec)?;

        self.cell.set(vec).map_err(|_| {
            ShellError::GenericError(
                "Failed to read binary data from pipe".to_string(),
                " ".to_string(),
                None,
                None,
                vec![],
            )
        })?;

        Ok(self.cell.get().unwrap().as_slice())
    }
}
