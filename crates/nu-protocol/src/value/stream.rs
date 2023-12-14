use serde::{Deserialize, Serialize};

use crate::*;
use std::{
    fmt::Debug,
    sync::{atomic::AtomicBool, Arc},
};

pub struct RawStream {
    pub stream: Box<dyn Iterator<Item = Result<Vec<u8>, ShellError>> + Send + 'static>,
    pub leftover: Vec<u8>,
    pub ctrlc: Option<Arc<AtomicBool>>,
    pub datatype: StreamDataType,
    pub span: Span,
    pub known_size: Option<u64>, // (bytes)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StreamDataType {
    Binary,
    Text,
}

impl RawStream {
    pub fn new(
        stream: Box<dyn Iterator<Item = Result<Vec<u8>, ShellError>> + Send + 'static>,
        ctrlc: Option<Arc<AtomicBool>>,
        span: Span,
        known_size: Option<u64>,
    ) -> Self {
        Self {
            stream,
            leftover: vec![],
            ctrlc,
            datatype: StreamDataType::Text,
            span,
            known_size,
        }
    }

    pub fn into_bytes(self) -> Result<Spanned<Vec<u8>>, ShellError> {
        let mut output = vec![];

        for item in self.stream {
            if nu_utils::ctrl_c::was_pressed(&self.ctrlc) {
                break;
            }
            output.extend(item?);
        }

        Ok(Spanned {
            item: output,
            span: self.span,
        })
    }

    pub fn into_string(self) -> Result<Spanned<String>, ShellError> {
        let mut output = String::new();
        let span = self.span;
        let ctrlc = &self.ctrlc.clone();

        for item in self {
            if nu_utils::ctrl_c::was_pressed(ctrlc) {
                break;
            }
            output.push_str(&item?.as_string()?);
        }

        Ok(Spanned { item: output, span })
    }

    pub fn chain(self, stream: RawStream) -> RawStream {
        RawStream {
            stream: Box::new(self.stream.chain(stream.stream)),
            leftover: self.leftover.into_iter().chain(stream.leftover).collect(),
            ctrlc: self.ctrlc,
            datatype: self.datatype,
            span: self.span,
            known_size: self.known_size,
        }
    }

    pub fn drain(self) -> Result<(), ShellError> {
        for next in self {
            match next {
                Ok(val) => {
                    if let Value::Error { error, .. } = val {
                        return Err(*error);
                    }
                }
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }
}
impl Debug for RawStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawStream").finish()
    }
}
impl Iterator for RawStream {
    type Item = Result<Value, ShellError>;

    fn next(&mut self) -> Option<Self::Item> {
        if nu_utils::ctrl_c::was_pressed(&self.ctrlc) {
            return None;
        }

        // If we know we're already binary, just output that
        match self.datatype {
            StreamDataType::Binary => self.stream.next().map(|buffer| {
                buffer.map(|mut v| {
                    if !self.leftover.is_empty() {
                        for b in self.leftover.drain(..).rev() {
                            v.insert(0, b);
                        }
                    }
                    Value::binary(v, self.span)
                })
            }),
            StreamDataType::Text => {
                // We *may* be text. We're only going to try utf-8. Other decodings
                // needs to be taken as binary first, then passed through `decode`.
                if let Some(buffer) = self.stream.next() {
                    match buffer {
                        Ok(mut v) => {
                            if !self.leftover.is_empty() {
                                v.reserve(self.leftover.len());
                                v.splice(0..0, self.leftover.drain(..));
                            }

                            match std::str::from_utf8(&v) {
                                Ok(s) => {
                                    // Great, we have a complete string, let's output it
                                    Some(Ok(Value::string(s, self.span)))
                                }
                                Err(err) => {
                                    // Okay, we *might* have a string but we've also got some errors
                                    if v.is_empty() {
                                        // We can just end here
                                        None
                                    } else if v.len() > 3 && (v.len() - err.valid_up_to() > 3) {
                                        // As UTF-8 characters are max 4 bytes, if we have more than that in error we know
                                        // that it's not just a character spanning two frames.
                                        // We now know we are definitely binary, so switch to binary and stay there.
                                        self.datatype = StreamDataType::Binary;
                                        Some(Ok(Value::binary(v, self.span)))
                                    } else {
                                        // Okay, we have a tiny bit of error at the end of the buffer. This could very well be
                                        // a character that spans two frames. Since this is the case, remove the error from
                                        // the current frame an dput it in the leftover buffer.
                                        self.leftover = v[err.valid_up_to()..].to_vec();

                                        let buf = v[0..err.valid_up_to()].to_vec();

                                        match String::from_utf8(buf) {
                                            Ok(s) => Some(Ok(Value::string(s, self.span))),
                                            Err(_) => {
                                                // Something is definitely wrong. Switch to binary, and stay there
                                                self.datatype = StreamDataType::Binary;
                                                Some(Ok(Value::binary(v, self.span)))
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => Some(Err(e)),
                    }
                } else if !self.leftover.is_empty() {
                    let output = Ok(Value::binary(self.leftover.clone(), self.span));
                    self.leftover.clear();

                    Some(output)
                } else {
                    None
                }
            }
        }
    }
}

impl std::io::Read for RawStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut total_read = 0;

        while total_read < buf.len() {
            match self.stream.next() {
                Some(Ok(mut v)) => {
                    if !self.leftover.is_empty() {
                        for b in self.leftover.drain(..).rev() {
                            v.insert(0, b);
                        }
                    }

                    let to_read = std::cmp::min(buf.len() - total_read, v.len());

                    buf[total_read..total_read + to_read].copy_from_slice(&v[0..to_read]);

                    if to_read < v.len() {
                        self.leftover = v[to_read..].to_vec();
                    }

                    total_read += to_read;
                }
                Some(Err(_)) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Error in stream",
                    ))
                }
                None => {
                    if !self.leftover.is_empty() {
                        let to_read = std::cmp::min(buf.len() - total_read, self.leftover.len());

                        buf[total_read..total_read + to_read]
                            .copy_from_slice(&self.leftover[0..to_read]);

                        if to_read < self.leftover.len() {
                            self.leftover = self.leftover[to_read..].to_vec();
                        } else {
                            self.leftover.clear();
                        }

                        total_read += to_read;
                    } else {
                        return Ok(total_read);
                    }
                }
            }
        }

        Ok(total_read)
    }
}

/// A potentially infinite stream of values, optionally with a mean to send a Ctrl-C signal to stop
/// the stream from continuing.
///
/// In practice, a "stream" here means anything which can be iterated and produce Values as it iterates.
/// Like other iterators in Rust, observing values from this stream will drain the items as you view them
/// and the stream cannot be replayed.
pub struct ListStream {
    pub stream: Box<dyn Iterator<Item = Value> + Send + 'static>,
    pub ctrlc: Option<Arc<AtomicBool>>,
}

impl ListStream {
    pub fn into_string(self, separator: &str, config: &Config) -> String {
        self.map(|x: Value| x.into_string(", ", config))
            .collect::<Vec<String>>()
            .join(separator)
    }

    pub fn drain(self) -> Result<(), ShellError> {
        for next in self {
            if let Value::Error { error, .. } = next {
                return Err(*error);
            }
        }
        Ok(())
    }

    pub fn from_stream(
        input: impl Iterator<Item = Value> + Send + 'static,
        ctrlc: Option<Arc<AtomicBool>>,
    ) -> ListStream {
        ListStream {
            stream: Box::new(input),
            ctrlc,
        }
    }
}

impl Debug for ListStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListStream").finish()
    }
}

impl Iterator for ListStream {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        if nu_utils::ctrl_c::was_pressed(&self.ctrlc) {
            None
        } else {
            self.stream.next()
        }
    }
}

#[cfg(test)]
mod tests {

    use super::RawStream;
    use std::io::Read;

    #[test]
    fn raw_stream_reads() {
        let mut stream = RawStream::new(
            Box::new(
                vec!["Hello", " ", "World", "!"]
                    .into_iter()
                    .map(|x| Ok(x.to_string().into_bytes())),
            ),
            None,
            crate::Span::unknown(),
            None,
        );

        let mut buf: Vec<u8> = vec![];

        stream.read_to_end(&mut buf).unwrap();

        assert_eq!(buf, "Hello World!".as_bytes());
    }

    #[test]
    fn raw_stream_with_leftover_reads() {
        let mut stream = RawStream::new(
            Box::new(
                vec!["Hello", " ", "World", "!"]
                    .into_iter()
                    .map(|x| Ok(x.to_string().into_bytes())),
            ),
            None,
            crate::Span::unknown(),
            None,
        );

        let mut buf: Vec<u8> = vec![];

        stream.leftover = "UwU ".as_bytes().to_vec();

        stream.read_to_end(&mut buf).unwrap();

        assert_eq!(buf, "UwU Hello World!".as_bytes());
    }
}
