use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use wormlang::CharStats;

const HELP: &str = "\
charstats - count the frequency of every UTF-8 character in a pile of text

USAGE:
    charstats [OPTIONS] [PATH]...

Reads text from the given files/directories (recursively). With no PATH it
reads every file under the crate's corpus/ folder. Pass - to read stdin.
Invalid UTF-8 bytes are replaced with U+FFFD.

OPTIONS:
    --top <N>    only print the N most frequent characters
    --out <FILE> also write the table to FILE
    -h, --help   show this help

OUTPUT (TSV):
    # total<TAB>n
    # distinct<TAB>m
    # count<TAB>codepoint<TAB>char
    count<TAB>U+XXXX<TAB>\"char\"
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("charstats: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut top: Option<usize> = None;
    let mut out: Option<PathBuf> = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "--top" => {
                let value = args.next().ok_or_else(|| invalid("--top needs a number"))?;
                top = Some(value.parse().map_err(|_| invalid("--top needs a number"))?);
            }
            "--out" => {
                let value = args.next().ok_or_else(|| invalid("--out needs a path"))?;
                out = Some(PathBuf::from(value));
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    let mut stats = CharStats::new();

    if paths.is_empty() {
        let corpus = default_corpus();
        if !corpus.is_dir() {
            return Err(invalid(&format!(
                "default corpus folder not found: {} (put text there, pass PATH..., or use - for stdin)",
                corpus.display()
            )));
        }
        paths.push(corpus);
    }

    let mut files: Vec<PathBuf> = Vec::new();
    let mut use_stdin = false;
    for path in &paths {
        if path.as_os_str() == "-" {
            use_stdin = true;
        } else {
            collect_files(path, &mut files)?;
        }
    }
    files.sort();
    for file in &files {
        let bytes = fs::read(file)?;
        stats.add_str(&String::from_utf8_lossy(&bytes));
    }
    if !files.is_empty() {
        eprintln!("charstats: read {} file(s)", files.len());
    }
    if use_stdin {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text)?;
        stats.add_str(&text);
    }

    let mut table = String::new();
    writeln!(table, "# total\t{}", stats.total()).unwrap();
    writeln!(table, "# distinct\t{}", stats.distinct()).unwrap();
    writeln!(table, "# count\tcodepoint\tchar").unwrap();

    let sorted = stats.sorted();
    let limit = top.unwrap_or(sorted.len()).min(sorted.len());
    for &(ch, count) in sorted.iter().take(limit) {
        writeln!(table, "{}\tU+{:04X}\t{:?}", count, ch as u32, ch).unwrap();
    }

    print!("{table}");

    if let Some(path) = &out {
        fs::write(path, table.as_bytes())?;
        eprintln!("charstats: wrote {}", path.display());
    }
    Ok(())
}

fn default_corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

fn collect_files(path: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_files(&entry?.path(), out)?;
        }
    } else {
        out.push(path.to_path_buf());
    }
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
