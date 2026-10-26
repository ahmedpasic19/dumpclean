//! Single streaming pass over the dump. Memory use is O(number of tables),
//! independent of file size: we only keep byte offsets, never the data.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

const HEAD_MAX: usize = 1024; // only the first bytes of each line are inspected
const BUF: usize = 8 << 20;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Table,
    View,
    Function,
    Procedure,
    Event,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Table => "TABLE",
            Kind::View => "VIEW",
            Kind::Function => "FUNCTION",
            Kind::Procedure => "PROCEDURE",
            Kind::Event => "EVENT",
        }
    }
}

/// A contiguous byte range of the dump. The whole file is covered by segments,
/// each either owned by an item (table, view, ...) or unowned (header/footer).
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub line: u64,  // 1-based number of the first line
    pub lines: u64, // number of lines in the segment
    pub owner: Option<usize>,
}

pub struct Item {
    pub kind: Kind,
    pub name: String,
    pub segs: Vec<usize>,
    pub lines: u64,
    pub bytes: u64,
    pub marked: bool,
}

pub struct Index {
    pub segments: Vec<Segment>,
    pub items: Vec<Item>,
    pub total_bytes: u64,
    pub total_lines: u64,
}

impl Index {
    fn is_removed(&self, s: &Segment) -> bool {
        s.owner.map_or(false, |o| self.items[o].marked)
    }

    /// Byte ranges that survive, adjacent ones merged.
    pub fn kept_ranges(&self) -> Vec<(u64, u64)> {
        let mut out: Vec<(u64, u64)> = Vec::new();
        for s in &self.segments {
            if self.is_removed(s) {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.1 == s.start => last.1 = s.end,
                _ => out.push((s.start, s.end)),
            }
        }
        out
    }

    pub fn first_removed_start(&self) -> Option<u64> {
        self.segments
            .iter()
            .find(|s| self.is_removed(s))
            .map(|s| s.start)
    }

    /// Maps a line number of the *cleaned* stream back to the owning item.
    /// Entries: (first line number in cleaned stream, owner).
    pub fn out_line_map(&self) -> Vec<(u64, Option<usize>)> {
        let mut map = Vec::new();
        let mut line = 1u64;
        for s in &self.segments {
            if self.is_removed(s) {
                continue;
            }
            map.push((line, s.owner));
            line += s.lines;
        }
        map
    }
}

struct LineScanner<R: BufRead> {
    r: R,
    offset: u64,
}

impl<R: BufRead> LineScanner<R> {
    /// Reads one line without allocating for it. Only the first HEAD_MAX bytes
    /// are copied into `head`, so a 50 MB INSERT line costs nothing extra.
    /// Returns (start offset, length) or None at EOF.
    fn next_line(&mut self, head: &mut Vec<u8>) -> io::Result<Option<(u64, u64)>> {
        head.clear();
        let start = self.offset;
        let mut len = 0u64;
        loop {
            let (take, found) = {
                let buf = self.r.fill_buf()?;
                if buf.is_empty() {
                    break;
                }
                let (take, found) = match memchr::memchr(b'\n', buf) {
                    Some(i) => (i + 1, true),
                    None => (buf.len(), false),
                };
                let room = HEAD_MAX.saturating_sub(head.len());
                head.extend_from_slice(&buf[..take.min(room)]);
                (take, found)
            };
            self.r.consume(take);
            len += take as u64;
            if found {
                break;
            }
        }
        if len == 0 {
            return Ok(None);
        }
        self.offset += len;
        Ok(Some((start, len)))
    }
}

fn trim_eol(mut s: &[u8]) -> &[u8] {
    while let Some((&l, rest)) = s.split_last() {
        if l == b'\n' || l == b'\r' || l == b' ' {
            s = rest;
        } else {
            break;
        }
    }
    s
}

/// Text between the first and last backtick.
fn bt_name(t: &[u8]) -> Option<String> {
    let a = t.iter().position(|&b| b == b'`')?;
    let b = t.iter().rposition(|&b| b == b'`')?;
    if b <= a {
        return None;
    }
    Some(String::from_utf8_lossy(&t[a + 1..b]).into_owned())
}

enum Boundary {
    /// A new item starts. bool: the `--` comment line just before belongs to it.
    Item(Kind, String, bool),
    /// Header/footer/section text that belongs to no item (never removed).
    Unowned(bool),
}

fn classify(t: &[u8]) -> Option<Boundary> {
    use Boundary::*;
    if t.starts_with(b"-- ") {
        if t.starts_with(b"-- Table structure for table ") {
            return bt_name(t).map(|n| Item(Kind::Table, n, true));
        }
        if t.starts_with(b"-- Temporary view structure for view ")
            || t.starts_with(b"-- Final view structure for view ")
        {
            return bt_name(t).map(|n| Item(Kind::View, n, true));
        }
        if t.starts_with(b"-- Dumping routines for database")
            || t.starts_with(b"-- Dumping events for database")
            || t.starts_with(b"-- Current Database:")
            || t.starts_with(b"-- Dump completed")
        {
            return Some(Unowned(true));
        }
    } else if t.starts_with(b"/*!") {
        if t.starts_with(b"/*!40103 SET TIME_ZONE=@OLD_TIME_ZONE") {
            return Some(Unowned(false)); // start of the mysqldump footer
        }
        if t.starts_with(b"/*!50003 DROP FUNCTION IF EXISTS") {
            return bt_name(t).map(|n| Item(Kind::Function, n, false));
        }
        if t.starts_with(b"/*!50003 DROP PROCEDURE IF EXISTS") {
            return bt_name(t).map(|n| Item(Kind::Procedure, n, false));
        }
        if t.starts_with(b"/*!50106 DROP EVENT IF EXISTS") {
            return bt_name(t).map(|n| Item(Kind::Event, n, false));
        }
    }
    None
}

fn close(
    segments: &mut Vec<Segment>,
    items: &mut [Item],
    start: u64,
    end: u64,
    line: u64,
    lines: u64,
    owner: Option<usize>,
) {
    if end <= start {
        return;
    }
    let idx = segments.len();
    segments.push(Segment {
        start,
        end,
        line,
        lines,
        owner,
    });
    if let Some(o) = owner {
        let it = &mut items[o];
        it.segs.push(idx);
        it.lines += lines;
        it.bytes += end - start;
    }
}

pub fn build_index(path: &Path, mut progress: impl FnMut(u64, u64)) -> io::Result<Index> {
    let file = File::open(path)?;
    let total = file.metadata()?.len();
    let mut sc = LineScanner {
        r: BufReader::with_capacity(BUF, file),
        offset: 0,
    };
    let mut head: Vec<u8> = Vec::with_capacity(HEAD_MAX);

    let mut segments: Vec<Segment> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    let mut lookup: HashMap<(Kind, String), usize> = HashMap::new();

    let (mut seg_start, mut seg_line, mut seg_owner) = (0u64, 1u64, None::<usize>);
    let (mut prev_start, mut prev_line, mut prev_dashes) = (0u64, 0u64, false);
    let mut line_no = 0u64;
    let mut last_report = 0u64;

    while let Some((start, _len)) = sc.next_line(&mut head)? {
        line_no += 1;
        let t = trim_eol(&head);

        if let Some(b) = classify(t) {
            let (new_owner, use_prev) = match b {
                Boundary::Item(kind, name, up) => {
                    let key = (kind, name.clone());
                    let id = *lookup.entry(key).or_insert_with(|| {
                        items.push(Item {
                            kind,
                            name,
                            segs: vec![],
                            lines: 0,
                            bytes: 0,
                            marked: false,
                        });
                        items.len() - 1
                    });
                    (Some(id), up)
                }
                Boundary::Unowned(up) => (None, up),
            };
            if !(new_owner.is_none() && seg_owner.is_none()) {
                // The `--` line above a marker is part of the new block.
                let (bstart, bline) = if use_prev
                    && prev_dashes
                    && prev_start >= seg_start
                    && prev_line >= seg_line
                {
                    (prev_start, prev_line)
                } else {
                    (start, line_no)
                };
                close(
                    &mut segments,
                    &mut items,
                    seg_start,
                    bstart,
                    seg_line,
                    bline - seg_line,
                    seg_owner,
                );
                seg_start = bstart;
                seg_line = bline;
                seg_owner = new_owner;
            }
        }

        prev_start = start;
        prev_line = line_no;
        prev_dashes = t == b"--";

        if sc.offset - last_report >= (32 << 20) {
            progress(sc.offset, total);
            last_report = sc.offset;
        }
    }
    close(
        &mut segments,
        &mut items,
        seg_start,
        sc.offset,
        seg_line,
        line_no + 1 - seg_line,
        seg_owner,
    );
    progress(total, total);

    Ok(Index {
        segments,
        items,
        total_bytes: sc.offset,
        total_lines: line_no,
    })
}
