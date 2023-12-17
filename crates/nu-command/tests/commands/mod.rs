mod alias;
mod all;
mod any;
mod append;
mod assignment;
mod break_;
mod cal;
mod cd;
mod compact;
mod complete;
mod config_env_default;
mod config_nu_default;
mod continue_;
mod conversions;
mod cp;
mod date;
mod debug_info;
mod def;
mod default;
mod detect_columns;
mod do_;
mod drop;
mod each;
mod echo;
mod empty;
mod error_make;
mod every;
mod exec;
mod export_def;
mod fill;
mod find;
mod first;
mod flatten;
mod for_;
#[cfg(feature = "extra")]
mod format;
mod generate;
mod get;
mod glob;
mod group_by;
mod hash_;
mod headers;
mod help;
mod histogram;
mod insert;
mod inspect;
mod into_datetime;
mod into_filesize;
mod into_int;
mod join;
mod last;
mod length;
mod let_;
mod lines;
mod ln;
mod loop_;
mod ls;
mod match_;
mod math;
mod merge;
mod mkdir;
mod mktemp;
mod move_;
mod mut_;
mod network;
mod nu_check;
mod open;
mod par_each;
mod parse;
mod path;
mod platform;
mod prepend;
mod print;
#[cfg(feature = "sqlite")]
mod query;
mod random;
mod range;
mod redirection;
mod reduce;
mod reject;
mod rename;
mod return_;
mod reverse;
mod rm;
#[cfg(feature = "extra")]
mod roll;
#[cfg(feature = "extra")]
mod rotate;
mod run_external;
mod save;
mod select;
mod semicolon;
mod seq;
mod seq_char;
mod skip;
mod sort;
mod sort_by;
mod source_env;
mod split_by;
mod split_column;
mod split_row;
mod str_;
mod table;
mod take;
mod terminal;
mod to_text;
mod touch;
mod transpose;
mod try_;
mod ucp;
#[cfg(unix)]
mod ulimit;
mod umkdir;
mod uniq;
mod uniq_by;
mod update;
mod upsert;
mod url;
mod use_;
mod where_;
#[cfg(feature = "which-support")]
mod which;
mod while_;
mod with_env;
mod wrap;
mod zip;
