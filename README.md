# dumpclean

Monorepo for **dumpclean** — a terminal tool to remove tables, views, routines, and events from huge single-file MySQL dumps without loading them into memory — plus a small landing site with download links.

| Package | Path | Description |
|---------|------|-------------|
| CLI | [`crates/dumpclean`](crates/dumpclean) | Rust TUI / non-interactive cleaner |
| Web | [`apps/web`](apps/web) | Next.js landing page (Vercel) |

## CLI (quick start)

```bash
cd crates/dumpclean
cargo build --release
# Windows: target\release\dumpclean.exe
# Unix:     target/release/dumpclean
```

Full usage, key bindings, and flags: [crates/dumpclean/README.md](crates/dumpclean/README.md).

## Web (local dev)

From the repo root:

```bash
npm install
npm run dev
```

See [apps/web/README.md](apps/web/README.md) for Vercel deployment and environment variables.

## Releases

1. Bump `version` in `crates/dumpclean/Cargo.toml`.
2. Commit and push to `main`.
3. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`
4. GitHub Actions builds Windows, Linux, and macOS (x64 + Apple Silicon) binaries and attaches them to the GitHub Release.
5. The landing page picks up the latest release automatically.

Pre-built downloads: [GitHub Releases](https://github.com/ahmedpasic19/dumpclean/releases).
