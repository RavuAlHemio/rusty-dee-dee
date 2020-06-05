use std::fmt::{Display, Formatter, Error as FmtError};
use clap::Clap;

#[derive(Clap)]
#[clap()]
pub struct Opts {
    #[clap(subcommand)]
    pub subcmd: Subcommand,
}

#[derive(Clap)]
pub enum Subcommand {
    #[cfg(target_os = "windows")]
    #[clap(about = "Lists available disks.")]
    List,

    #[clap(about = "Perform a copy.")]
    DD(DDOptions),
}

#[derive(Clap)]
pub struct DDOptions {
    #[clap(required = true, about = "File to read from.")]
    pub source: String,

    #[clap(required = true, about = "File to write to.")]
    pub destination: String,

    #[clap(short = "s", long = "src-skip" , value_names = &["BYTES"], default_value = "0", about = "Number of bytes to skip in the source file before copying.")]
    pub src_skip: u64,

    #[clap(short = "S", long = "dest-skip", value_names = &["BYTES"], default_value = "0", about = "Number of bytes to skip in the destination file before copying.")]
    pub dest_skip: u64,

    #[clap(short = "c", long = "count", value_names = &["BYTES"], about = "Number of bytes to copy.")]
    pub count: Option<u64>,

    #[clap(short = "b", long = "block-size", value_names = &["BYTES"], default_value = "4194304", min_values = 1, about = "Size of each block in bytes when copying.")]
    pub block_size: usize,

    #[clap(short = "C", long = "create-dest", about = "Allow creation of the destination file if it does not already exist.")]
    pub create_dest: bool,

    #[clap(short = "t", long = "truncate-dest", about = "Truncate the destination file before copying.")]
    pub truncate_dest: bool,

    #[clap(short = "x", long = "src-excl", about = "Open the source file with exclusive access.")]
    pub src_excl: bool,

    #[clap(short = "X", long = "dest-excl", about = "Open the destination file with exclusive access.")]
    pub dest_excl: bool,

    #[clap(short = "R", long = "dest-read", about = "Open the destination file with read access in addition to write access.")]
    pub dest_read: bool,
}
