use nu_plugin::{serve_plugin, MsgPackSerializer};
use nu_plugin_file::FileCmd;

fn main() {
    serve_plugin(&mut FileCmd, MsgPackSerializer {})
}
