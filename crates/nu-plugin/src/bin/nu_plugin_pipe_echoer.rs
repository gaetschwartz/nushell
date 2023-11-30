use nu_plugin::OsPipe;
use std::io::Read;

fn main() {
    let serialized = std::env::args().nth(1).unwrap();
    let deserialized: OsPipe = serde_json::from_str(&serialized).unwrap();
    let mut reader = deserialized.open_read();

    let mut buf = [0u8; 11];

    _ = reader.read(&mut buf).unwrap();

    println!("{}", std::str::from_utf8(&buf).unwrap());
}
