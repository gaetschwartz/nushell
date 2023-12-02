use std::io::Read;

use nu_pipes::unidirectional::{PipeRead, UnOpenedPipe};

fn main() {
    let serialized = std::env::args().nth(1).unwrap();
    let deserialized: UnOpenedPipe<PipeRead> = serde_json::from_str(&serialized).unwrap();
    let mut reader = deserialized.open().unwrap();

    let mut buf = [0u8; 11];

    _ = reader.read(&mut buf).unwrap();

    println!("{}", std::str::from_utf8(&buf).unwrap());
}
