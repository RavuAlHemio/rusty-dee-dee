mod opts;
#[cfg(target_os = "windows")]
mod winvol;


use std::convert::TryInto;
use std::env;
use std::io::{Read, Seek, SeekFrom, Write};
use std::fs::OpenOptions;
use std::process::exit;

#[cfg(target_os = "windows")]
use std::os::windows::fs::OpenOptionsExt;

use clap::derive::Clap;

use crate::opts::{DDOptions, Opts, Subcommand};


#[cfg(target_os = "windows")]
fn do_list_windows() -> i32 {
    let disks_res = winvol::get_windows_disks();
    if let Err(err) = disks_res {
        eprintln!("failed to obtain disks: {}", err);
        return 1;
    }
    let disks = disks_res.unwrap();

    for disk in &disks {
        // try opening in turn to obtain size
        let size = {
            let opened_res = OpenOptions::new()
                .read(true)
                .open(disk);
            if let Err(err) = opened_res {
                eprintln!("failed to open disk {} to obtain size: {}", disk, err);
                None
            } else {
                let size_res = winvol::get_disk_size(&opened_res.unwrap());
                if let Err(err) = size_res {
                    eprintln!("failed to obtain size of disk {}: {}", disk, err);
                    None
                } else {
                    Some(size_res.unwrap())
                }
            }
        };

        println!("{}", disk);
        if let Some(sz) = size {
            println!("    {}", sz);
        }
        println!();
    }

    0
}

fn do_dd(args: &DDOptions) -> i32 {
    let mut source_file_options = OpenOptions::new();
    source_file_options
        .read(true);
    if cfg!(target_os = "windows") && args.src_excl {
        source_file_options.share_mode(0);
    }
    let source_file_res = source_file_options
        .open(&args.source);
    if let Err(err) = source_file_res {
        eprintln!("failed to open source file: {}", err);
        return 1;
    }
    let mut source_file = source_file_res.unwrap();
    if args.src_skip > 0 {
        let seek_res = source_file.seek(SeekFrom::Start(args.src_skip));
        if let Err(err) = seek_res {
            eprintln!("failed to seek in source file: {}", err);
            return 1;
        }
    }

    let mut dest_file_options = OpenOptions::new();
    dest_file_options
        .read(args.dest_read)
        .write(true)
        .truncate(args.truncate_dest)
        .create(args.create_dest);
    if cfg!(target_os = "windows") && args.dest_excl {
        dest_file_options.share_mode(0);
    }
    let dest_file_res = dest_file_options
        .open(&args.destination);
    if let Err(err) = dest_file_res {
        eprintln!("failed to open destination file: {}", err);
        return 1;
    }
    let mut dest_file = dest_file_res.unwrap();
    if args.dest_skip > 0 {
        let seek_res = dest_file.seek(SeekFrom::Start(args.dest_skip));
        if let Err(err) = seek_res {
            eprintln!("failed to seek in destination file: {}", err);
            return 1;
        }
    }

    println!();

    let mut buf = vec![0u8; args.block_size];
    let mut remaining_bytes: u64 = args.count.unwrap_or(u64::max_value());
    let block_size_u64: u64 = args.block_size.try_into().unwrap();
    while remaining_bytes > 0 {
        let count_to_read: usize = if remaining_bytes > block_size_u64 {
            args.block_size
        } else {
            remaining_bytes.try_into().unwrap()
        };

        let read_count = match source_file.read(&mut buf[0..count_to_read]) {
            Ok(rc) => rc,
            Err(err) => {
                eprintln!("failed to read {} bytes from source file: {}", count_to_read, err);
                return 1;
            },
        };
        let read_count_u64: u64 = read_count.try_into().unwrap();
        remaining_bytes -= read_count_u64;

        print!("\r{} bytes remain", remaining_bytes);
        let _ = std::io::stdout().flush();

        if read_count == 0 {
            break;
        }

        let write_count = match dest_file.write(&buf[0..read_count]) {
            Ok(wc) => wc,
            Err(err) => {
                eprintln!("failed to write {} bytes to destination file: {}", read_count, err);
                return 1;
            },
        };
        if write_count != read_count {
            eprintln!("number of bytes read ({}) does not match number of bytes written ({})", read_count, write_count);
            return 1;
        }
    }

    0
}

fn do_main() -> i32 {
    let args: Vec<String> = env::args().collect();
    let opts: Opts = match Opts::try_parse_from(args) {
        Ok(o) => o,
        Err(err) => {
            eprint!("{}", err);
            return 1;
        },
    };

    if cfg!(target_os = "windows") {
        if let Subcommand::List = opts.subcmd {
            return do_list_windows();
        }
    }

    if let Subcommand::DD(dd_options) = opts.subcmd {
        do_dd(&dd_options)
    } else {
        unreachable!("unhandled subcommand");
    }
}

fn main() {
    exit(do_main());
}
