use clap::Parser;
use indicatif::{MultiProgress, ProgressBar};
use indicatif_log_bridge::LogWrapper;
use std::cmp::min;
use std::fmt::{Debug, Display, Formatter};
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use dialoguer::Confirm;

/// KISS CLI tool that zeroes out a file before deleting it (for privacy concerns)
#[derive(Parser, Debug)]
#[command(version, about, author, long_about = None)]
struct Cli {
    /// Path to the file to blank
    #[arg()]
    file: PathBuf,

    /// Recursively blank files
    #[arg(short, long, alias = "R", default_value_t = false)]
    recursive: bool,

    /// Force the removal of files without prompts
    #[arg(short, long, default_value_t = false)]
    force: bool,

    /// Whether to follow symbolic and hard links in the blanking process
    #[arg(long, default_value_t = false)]
    follow_links: bool,

    /// Whether to keep the file after blanking it
    #[arg(short, long, default_value_t = false)]
    keep_file: bool,

    /// The size of the write buffer in bytes
    #[arg(short, long, default_value_t = 4096)]
    buffer_size: u16,
}

struct FileMeta {
    is_symlink: bool,
    is_dir: bool,
    // is_file: bool,
    bytes_size: usize,
}

impl From<fs::Metadata> for FileMeta {
    fn from(m: fs::Metadata) -> Self {
        Self{
            is_dir: m.is_dir(),
            is_symlink: m.is_symlink(),
            // is_file: m.is_file(),
            bytes_size: m.len() as usize,
        }
    }
}

fn path_to_str(path: &PathBuf) -> &str {
    path.as_path().to_str().unwrap()
}

fn main() {
    let multi_progress = make_progressbar();
    let args = Cli::parse();
    log::debug!("{:?}", &args);

    let spinner = multi_progress.add(ProgressBar::new_spinner());
    let result = process_file(&args, &args.file);

    match result {
        Ok(_) => {
            spinner.finish();
            std::process::exit(exitcode::OK);
        }
        Err(e) => {
            spinner.finish_and_clear();
            log::error!("Error: {}", e);
            std::process::exit(exitcode::DATAERR);
        }
    }
}

fn make_progressbar() -> MultiProgress {
    let logger =
        env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("info"))
            .build();
    let level = logger.filter();
    let multi = MultiProgress::new();

    LogWrapper::new(multi.to_owned(), logger)
        .try_init()
        .unwrap();
    log::set_max_level(level);
    multi
}

fn process_file(args: &Cli, path: &PathBuf) -> std::io::Result<()> {
    let should_process = args.force || Confirm::new()
        .with_prompt(format!("Blank {} ?", path_to_str(path)))
        .default(true)
        .wait_for_newline(true)
        .interact()?;

    if !should_process {
        return Ok(())
    }

    let meta: FileMeta = fs::metadata(path)?.into();

    if meta.is_symlink && !args.follow_links {
        return Ok(())
    }

    if meta.is_dir {
        return if args.recursive {
            for entry in fs::read_dir(path)? {
                let entry_path = entry?.path();
                process_file(&args, &entry_path)?;
            }

            if !args.keep_file {
                fs::remove_dir(path)
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    let file = File::options().write(true).read(true).open(path)?;

    if let Ok(_) = file.lock() {
        let file_size_bytes = meta.bytes_size;
        let buff_size = min(args.buffer_size as usize, file_size_bytes);

        let mut writer = BufWriter::with_capacity(buff_size, file);
        let mut offset = 0usize;
        let mut bytes = vec![];
        bytes.resize(buff_size, 0u8);

        while offset+buff_size < file_size_bytes {
            let bytes_written = writer.write(bytes.by_ref())?;
            writer.flush()?;
            offset = offset + bytes_written;
        }

        if offset < file_size_bytes {
            let mut bytes = vec![];
            bytes.resize(file_size_bytes - offset, 0u8);
            writer.write(bytes.by_ref())?;
            writer.flush()?;
        }
    } else {
        return Err(std::io::ErrorKind::ResourceBusy.into())
    }

    if !args.keep_file {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}


