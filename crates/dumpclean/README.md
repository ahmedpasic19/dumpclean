# dumpclean (CLI)

Terminal tool to remove tables / views / functions / procedures / events from huge
single-file MySQL dumps without loading them into memory.

## Build

1. Install Rust: https://rustup.rs
2. From this directory: `cargo build --release`
3. Binary: `target/release/dumpclean` (`.exe` on Windows)

## Run

    dumpclean dump.sql
    dumpclean dump.sql --mysql-args "-h 127.0.0.1 -u root mydb"   (enables test import, key t)

Password: `set MYSQL_PWD=secret` before starting (Windows), or add `--defaults-extra-file=C:\path\my.cnf` to `--mysql-args`.

## Keys

| Key | Action |
|-----|--------|
| ↑↓ PgUp PgDn Home End | move |
| Space | mark/unmark (moves down) |
| a | mark all visible |
| u | unmark all |
| / | filter by name |
| s | cycle sort: file order, size, lines, name |
| t | test import |
| r | show last test results |
| w | write |
| q | quit |

Source of truth: `src/keys.rs` (also shown in `dumpclean --help` and the TUI footer).

## Non-interactive

    dumpclean dump.sql --list
    dumpclean dump.sql --remove tbl1,tbl2 --out cleaned.sql
    dumpclean dump.sql --remove tbl1,tbl2 --in-place
    dumpclean dump.sql --test --exclude tbl1 --mysql-args "-u root mydb"
