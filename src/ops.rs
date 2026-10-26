//! Writing the cleaned dump and the "test import" run.

use crate::index::Index;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{ChildStderr, Command, Stdio};
use std::thread;

const CHUNK: usize = 8 << 20;

/// Copies the `keep` ranges from `r` to `w`, starting at offset `wpos` in `w`.
/// Safe for in-place use (r and w on the same file) because we only ever
/// move data towards the start of the file.
fn compact(
    r: &mut File,
    w: &mut File,
    keep: &[(u64, u64)],
    mut wpos: u64,
    mut progress: impl FnMut(u64, u64),
) -> io::Result<u64> {
    let total: u64 = keep.iter().map(|(s, e)| e - s).sum();
    let mut buf = vec![0u8; CHUNK];
    let mut done = 0u64;
    for &(s, e) in keep {
        r.seek(SeekFrom::Start(s))?;
        w.seek(SeekFrom::Start(wpos))?;
        let mut left = e - s;
        while left > 0 {
            let n = buf.len().min(left as usize);
            r.read_exact(&mut buf[..n])?;
            w.write_all(&buf[..n])?;
            left -= n as u64;
            wpos += n as u64;
            done += n as u64;
            progress(done, total);
        }
    }
    Ok(wpos)
}

pub fn write_new(src: &Path, dst: &Path, idx: &Index, progress: impl FnMut(u64, u64)) -> io::Result<u64> {
    if src == dst {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "output path equals input path"));
    }
    let mut r = File::open(src)?;
    let mut w = File::create(dst)?;
    let end = compact(&mut r, &mut w, &idx.kept_ranges(), 0, progress)?;
    w.set_len(end)?;
    w.sync_all()?;
    Ok(end)
}

/// Rewrites the file itself. Everything before the first removed block is
/// untouched; everything after it is shifted down, then the file is truncated.
pub fn write_in_place(src: &Path, idx: &Index, progress: impl FnMut(u64, u64)) -> io::Result<u64> {
    let first = idx
        .first_removed_start()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "nothing to remove"))?;
    let keep: Vec<(u64, u64)> = idx.kept_ranges().into_iter().filter(|&(s, _)| s >= first).collect();
    let mut r = File::open(src)?;
    let mut w = OpenOptions::new().write(true).open(src)?;
    let end = compact(&mut r, &mut w, &keep, first, progress)?;
    w.set_len(end)?;
    w.sync_all()?;
    Ok(end)
}

pub struct ErrGroup {
    pub owner: Option<usize>, // None = outside any table/view/routine
    pub count: u64,
    pub first: String,
}

pub struct TestResult {
    pub groups: Vec<ErrGroup>,
    pub other: Vec<String>,
    pub aborted: bool,
    pub exit_ok: bool,
}

/// "ERROR 1215 (HY000) at line 123: msg" -> (123, "1215 (HY000): msg")
fn parse_err(s: &str) -> Option<(u64, String)> {
    let rest = s.strip_prefix("ERROR ")?;
    let at = rest.find(" at line ")?;
    let after = &rest[at + 9..];
    let colon = after.find(':')?;
    let line: u64 = after[..colon].trim().parse().ok()?;
    Some((line, format!("{}: {}", &rest[..at], after[colon + 1..].trim())))
}

fn collect_errors(stderr: ChildStderr, map: Vec<(u64, Option<usize>)>) -> (Vec<ErrGroup>, Vec<String>) {
    let mut groups: Vec<ErrGroup> = Vec::new();
    let mut pos: HashMap<Option<usize>, usize> = HashMap::new();
    let mut other: Vec<String> = Vec::new();
    for raw in BufReader::new(stderr).split(b'\n').map_while(Result::ok) {
        let text = String::from_utf8_lossy(&raw);
        let s = text.trim();
        if s.is_empty() {
            continue;
        }
        match parse_err(s) {
            Some((line, msg)) => {
                let k = map.partition_point(|&(st, _)| st <= line);
                let owner = if k == 0 { None } else { map[k - 1].1 };
                let gi = *pos.entry(owner).or_insert_with(|| {
                    groups.push(ErrGroup { owner, count: 0, first: msg.clone() });
                    groups.len() - 1
                });
                groups[gi].count += 1;
            }
            None => {
                if other.len() < 50 {
                    other.push(s.to_string());
                }
            }
        }
    }
    (groups, other)
}

/// Streams the (currently marked-for-removal-excluded) dump into `mysql --force`
/// and groups every error by the table/view/routine it happened in.
pub fn test_import(
    src: &Path,
    idx: &Index,
    mysql_args: &[String],
    mut progress: impl FnMut(u64, u64),
) -> io::Result<TestResult> {
    let keep = idx.kept_ranges();
    let total: u64 = keep.iter().map(|(s, e)| e - s).sum();
    let map = idx.out_line_map();

    let mut child = Command::new("mysql")
        .args(mysql_args)
        .arg("--force")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("could not start `mysql`: {e} (is mysql.exe on PATH?)")))?;

    let stderr = child.stderr.take().unwrap();
    let reader = thread::spawn(move || collect_errors(stderr, map));

    let mut stdin = child.stdin.take().unwrap();
    let mut r = File::open(src)?;
    let mut buf = vec![0u8; 4 << 20];
    let (mut done, mut aborted) = (0u64, false);
    'outer: for (s, e) in keep {
        r.seek(SeekFrom::Start(s))?;
        let mut left = e - s;
        while left > 0 {
            let n = buf.len().min(left as usize);
            r.read_exact(&mut buf[..n])?;
            if stdin.write_all(&buf[..n]).is_err() {
                aborted = true; // mysql exited early (bad credentials, ...)
                break 'outer;
            }
            left -= n as u64;
            done += n as u64;
            progress(done, total);
        }
    }
    drop(stdin);
    let status = child.wait()?;
    let (groups, other) = reader
        .join()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "stderr reader thread panicked"))?;
    Ok(TestResult { groups, other, aborted, exit_ok: status.success() })
}
