use nu_plugin::OsPipe;
use std::io::Read;

fn main() {
    let serialized = std::env::args().nth(1).unwrap();
    let deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();
    let mut reader = deserialized.reader();

    let mut buf = [0u8; 11];

    _ = reader.read(&mut buf).unwrap();

    eprintln!("{}", std::str::from_utf8(&buf).unwrap());
}
