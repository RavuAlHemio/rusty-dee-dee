use std::convert::TryInto;
use std::env;
use std::io::{Read, Seek, SeekFrom, Write};
use std::fs::OpenOptions;
use std::process::exit;
use std::str::FromStr;

#[cfg(target_os = "windows")]
use std::os::windows::fs::OpenOptionsExt;

use getopts::{Matches, Options};

#[cfg(target_os = "windows")]
mod winvol;

fn output_global_usage(prog_name: &str) {
    eprintln!("Usage: {} COMMAND [ARGS]", prog_name);
    eprintln!("");
    eprintln!("Commands:");

    if cfg!(target_os = "windows") {
        eprintln!("");
        eprintln!("  list    List disks available on the system.");
    }

    eprintln!("");
    eprintln!("  dd      Copy data.");

    eprintln!("");
    eprintln!("To obtain usage information about a command, call it and pass --help");
    eprintln!("as an argument.");
}

fn output_dd_usage(prog_name: &str, opts: &Options) {
    let usage = opts.usage(&format!("Usage: {} dd [OPTION...] SOURCE TARGET", prog_name));
    eprintln!("{}", usage);
}

#[cfg(target_os = "windows")]
fn do_list_windows(prog_name: &str, args: &Vec<String>) -> i32 {
    if args.len() != 2 {
        eprintln!("Usage: {} list", prog_name);
        return 1;
    }

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

fn get_arg_num<T: FromStr>(matches: &Matches, arg: &str, default: T) -> Result<T, <T as std::str::FromStr>::Err> {
    let str_opt: Option<String> = matches.opt_str(arg);
    if str_opt.is_none() {
        return Ok(default);
    }
    T::from_str(&str_opt.unwrap())
}

fn do_dd(prog_name: &str, raw_args: &Vec<String>) -> i32 {
    let mut opts = Options::new();
    opts.optopt("s", "src-skip", "Number of bytes to skip in the source file before copying.", "BYTES");
    opts.optopt("S", "dest-skip", "Number of bytes to skip in the destination file before copying.", "BYTES");
    opts.optopt("c", "count", "Number of bytes to copy.", "BYTES");
    opts.optopt("b", "block-size", "Size of each block in bytes when copying.", "BYTES");
    opts.optflag("C", "create-dest", "Allow creation of the destination file.");
    opts.optflag("t", "truncate-dest", "Allow truncation of the destination file.");
    opts.optflag("x", "src-excl", "Open source file with exclusive access.");
    opts.optflag("X", "dest-excl", "Open destination file with exclusive access.");
    opts.optflag("R", "dest-read", "Open destination file with read access in addition to write access.");
    let args_res = opts.parse(&raw_args[2..]);
    if args_res.is_err() {
        output_dd_usage(prog_name, &opts);
        return 1;
    }
    let args = args_res.unwrap();

    if args.free.len() != 2 {
        output_dd_usage(prog_name, &opts);
        return 1;
    }

    let source_skip_res = get_arg_num(&args, "s", 0);
    if source_skip_res.is_err() {
        eprintln!("invalid skip value");
        output_dd_usage(prog_name, &opts);
        return 1;
    }
    let source_skip: u64 = source_skip_res.unwrap();

    let dest_skip_res = get_arg_num(&args, "S", 0);
    if dest_skip_res.is_err() {
        eprintln!("invalid skip value");
        output_dd_usage(prog_name, &opts);
        return 1;
    }
    let dest_skip: u64 = dest_skip_res.unwrap();

    let count_res = get_arg_num(&args, "c", u64::max_value());
    if count_res.is_err() {
        eprintln!("invalid count value");
        output_dd_usage(prog_name, &opts);
        return 1;
    }
    let count: u64 = count_res.unwrap();

    let block_size_res = get_arg_num(&args, "b", 4*1024*1024);
    if block_size_res.is_err() {
        eprintln!("invalid block size value");
        output_dd_usage(prog_name, &opts);
        return 1;
    }
    let block_size: usize = block_size_res.unwrap();
    if block_size < 1 {
        eprintln!("block size is {}; must be at least 1", block_size);
        output_dd_usage(prog_name, &opts);
        return 1;
    }

    let allow_create_dest = args.opt_present("C");
    let truncate_dest = args.opt_present("t");
    let src_excl = args.opt_present("x");
    let dest_excl = args.opt_present("X");
    let dest_read = args.opt_present("R");

    let source_path = args.free.get(0).unwrap();
    let dest_path = args.free.get(1).unwrap();

    let mut source_file_options = OpenOptions::new();
    source_file_options
        .read(true);
    if cfg!(target_os = "windows") {
        if src_excl {
            source_file_options.share_mode(0);
        }
    }
    let source_file_res = source_file_options
        .open(source_path);
    if let Err(err) = source_file_res {
        eprintln!("failed to open source file: {}", err);
        return 1;
    }
    let mut source_file = source_file_res.unwrap();
    if source_skip > 0 {
        let seek_res = source_file.seek(SeekFrom::Start(source_skip));
        if let Err(err) = seek_res {
            eprintln!("failed to seek in source file: {}", err);
            return 1;
        }
    }

    let mut dest_file_options = OpenOptions::new();
    dest_file_options
        .read(dest_read)
        .write(true)
        .truncate(truncate_dest)
        .create(allow_create_dest);
    if cfg!(target_os = "windows") {
        if dest_excl {
            dest_file_options.share_mode(0);
        }
    }
    let dest_file_res = dest_file_options
        .open(dest_path);
    if let Err(err) = dest_file_res {
        eprintln!("failed to open destination file: {}", err);
        return 1;
    }
    let mut dest_file = dest_file_res.unwrap();
    if dest_skip > 0 {
        let seek_res = dest_file.seek(SeekFrom::Start(dest_skip));
        if let Err(err) = seek_res {
            eprintln!("failed to seek in destination file: {}", err);
            return 1;
        }
    }

    println!();

    let mut buf = vec![0u8; block_size];
    let mut remaining_bytes: u64 = count;
    let block_size_u64: u64 = block_size.try_into().unwrap();
    while remaining_bytes > 0 {
        let count_to_read: usize = if remaining_bytes > block_size_u64 {
            block_size
        } else {
            remaining_bytes.try_into().unwrap()
        };

        let read_count_res = source_file.read(&mut buf[0..count_to_read]);
        if let Err(err) = read_count_res {
            eprintln!("failed to read {} bytes from source file: {}", count_to_read, err);
            return 1;
        }
        let read_count: usize = read_count_res.unwrap();
        let read_count_u64: u64 = read_count.try_into().unwrap();
        remaining_bytes -= read_count_u64;

        print!("\r{} bytes remain", remaining_bytes);
        let _ = std::io::stdout().flush();

        if read_count == 0 {
            break;
        }

        let write_count_res = dest_file.write(&buf[0..read_count]);
        if let Err(err) = write_count_res {
            eprintln!("failed to write {} bytes to destination file: {}", read_count, err);
            return 1;
        }
        let write_count = write_count_res.unwrap();
        if write_count != read_count {
            eprintln!("number of bytes read ({}) does not match number of bytes written ({})", read_count, write_count);
            return 1;
        }
    }

    0
}

fn do_main() -> i32 {
    let args: Vec<String> = env::args().collect();
    let mut prog_name: String = "rusty-dee-dee".to_owned();
    if args.len() > 0 {
        prog_name = args.get(0).unwrap().to_owned();
    }

    if args.len() == 1 {
        output_global_usage(&prog_name);
        return 1;
    }

    let cmd = args.get(1).unwrap();
    if cmd == "--help" {
        output_global_usage(&prog_name);
        0
    } else if cmd == "list" {
        if cfg!(target_os = "windows") {
            do_list_windows(&prog_name, &args)
        } else {
            eprintln!("The 'list' command is only supported on Windows. Please search /dev");
            eprintln!("for the device you wish to target.");
            1
        }
    } else if cmd == "dd" {
        do_dd(&prog_name, &args)
    } else {
        eprintln!("unknown command {:?}", cmd);
        output_global_usage(&prog_name);
        1
    }
}

fn main() {
    exit(do_main());
}
