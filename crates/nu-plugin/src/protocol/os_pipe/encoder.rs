use serde::{Deserialize, Serialize};

use super::big_array::BoxedBigArray;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PipeChunk {
    Data(#[serde(with = "BoxedBigArray")] Box<[u8; 256]>),
    End,
}

trait ReadToPipeChunk {
    fn read_to_pipe_chunk(&mut self) -> std::io::Result<PipeChunk>;
}

impl<R: std::io::Read> ReadToPipeChunk for R {
    fn read_to_pipe_chunk(&mut self) -> std::io::Result<PipeChunk> {
        bincode::deserialize_from(self).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to deserialize pipe chunk: {}", e),
            )
        })
    }
}

trait WriteFromPipeChunk {
    fn write_from_pipe_chunk(&mut self, chunk: PipeChunk) -> std::io::Result<()>;
}

impl<W: std::io::Write> WriteFromPipeChunk for W {
    fn write_from_pipe_chunk(&mut self, chunk: PipeChunk) -> std::io::Result<()> {
        bincode::serialize_into(self, &chunk).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to serialize pipe chunk: {}", e),
            )
        })
    }
}

#[test]
fn test_pipe_chunk() {
    let mut buf = Vec::new();
    buf.write_from_pipe_chunk(PipeChunk::Data(Box::new([1; 256])))
        .unwrap();
    buf.write_from_pipe_chunk(PipeChunk::End).unwrap();

    let mut buf = buf.as_slice();
    assert_eq!(
        buf.read_to_pipe_chunk().unwrap(),
        PipeChunk::Data(Box::new([1; 256]))
    );
    assert_eq!(buf.read_to_pipe_chunk().unwrap(), PipeChunk::End);
}
