# nu_plugin_formats_v1

A nushell plugin to convert data to nushell tables.

# warning

This plugin is voluntarily using an older version of Nushell in order to test backward compatibility.

# support commands:

1. from eml - original ported from nushell core.
2. from ics - original ported from nushell core.
3. from ini - original ported from nushell core.
4. from vcf - original ported from nushell core.

# Prerequisite

`nushell`, It's a nushell plugin, so you need it.

# Usage

1. compile the binary: `cargo build`
2. register plugin(assume it's compiled in ./target/debug/):

```
register ./target/debug/nu_plugin_formats_v1
```
