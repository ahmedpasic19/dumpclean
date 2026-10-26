//! Interactive TUI key bindings (reference for UI, `--help`, and README).

/// One-line hint shown at the bottom of the main screen.
pub const FOOTER: &str = "\
 ↑↓ PgUp PgDn Home End move · Space mark (↓) · a all visible · u unmark all · \
/ filter · s sort · t test · r results · w write · q quit";

/// Printed after `dumpclean --help`.
pub const USAGE_KEYS: &str = "
KEYS (interactive):
  ↑↓ PgUp PgDn Home End   move
  Space                   mark/unmark (moves down)
  a                       mark all visible
  u                       unmark all
  /                       filter by name
  s                       cycle sort: file order, size, lines, name
  t                       test import (--mysql-args required)
  r                       show last test results
  w                       write (save cleaned dump)
  q                       quit
";
