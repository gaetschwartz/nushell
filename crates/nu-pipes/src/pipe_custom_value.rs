use nu_protocol::{CustomValue, ShellError, Span, Spanned, StreamDataType, Value};
use serde::{Deserialize, Serialize};
use std::{io::Read, sync::OnceLock};

use crate::{
    unidirectional::{Pipe, PipeRead, UnOpenedPipe},
    PipeReader,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct StreamCustomValue {
    pub span: Span,
    #[serde(skip, default)]
    pub data: OnceLock<Vec<u8>>,
    #[serde(skip, default)]
    pipe: OnceLock<UnOpenedPipe<PipeRead>>,
    datatype: StreamDataType,
}

impl StreamCustomValue {
    pub fn new(os_pipe: UnOpenedPipe<PipeRead>, span: Span) -> Self {
        Self {
            span,
            datatype: os_pipe.datatype,
            pipe: OnceLock::from(os_pipe),
            data: OnceLock::new(),
        }
    }

    fn read_binary(&self) -> Result<&Vec<u8>, ShellError> {
        if let Some(cell) = self.data.get() {
            Ok(cell)
        } else if let Some(pipe) = self.pipe.get() {
            let vec = read_pipe(pipe)?;

            return Ok(self.data.get_or_init(|| vec));
        } else {
            return Err(ShellError::GenericError(
                "Failed to read binary data from pipe".to_string(),
                " ".to_string(),
                None,
                None,
                vec![],
            ));
        }
    }
}

fn read_pipe(pipe: &UnOpenedPipe<PipeRead>) -> Result<Vec<u8>, ShellError> {
    let mut reader = pipe.open()?;
    let mut vec = Vec::new();
    _ = reader.read_to_end(&mut vec)?;
    Ok(vec)
}

impl CustomValue for StreamCustomValue {
    fn clone_value(&self, span: Span) -> Value {
        if let Some(cell) = self.data.get() {
            Value::binary(cell.to_vec(), span)
        } else if let Some(pipe) = self.pipe.get() {
            let vec = read_pipe(pipe).expect("Failed to read pipe");
            Value::custom_value(
                Box::new(Self {
                    span,
                    data: OnceLock::from(vec),
                    pipe: OnceLock::new(),
                    datatype: self.datatype,
                }),
                span,
            )
        } else {
            Value::error(
                ShellError::GenericError(
                    "Failed to clone custom value".to_string(),
                    " ".to_string(),
                    None,
                    None,
                    vec![],
                ),
                span,
            )
        }
    }

    fn value_string(&self) -> String {
        self.typetag_name().to_string()
    }

    fn to_base_value(&self, span: Span) -> Result<Value, ShellError> {
        let vec = self.as_binary()?;

        match self.datatype {
            StreamDataType::Binary => Ok(Value::binary(vec, span)),
            StreamDataType::Text => Ok(Value::string(String::from_utf8_lossy(vec), span)),
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
        "pipe"
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
        Ok(self.read_binary()?)
    }
}
