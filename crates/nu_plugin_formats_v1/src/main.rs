use nu_plugin::{serve_plugin, MsgPackSerializer};
use nu_plugin_formats_v1::FromCmds;

fn main() {
    serve_plugin(&mut FromCmds, MsgPackSerializer {})
}
