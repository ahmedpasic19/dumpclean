# dumpclean landing (Next.js)

Marketing page for the dumpclean CLI: short description, feature list, and download links from the latest [GitHub Release](https://github.com/ahmedpasic19/dumpclean/releases).

## Local development

From the **repository root**:

```bash
npm install
npm run dev
```

Open [http://localhost:3000](http://localhost:3000).

You can also run commands inside this folder with `npm run dev` if dependencies are installed via the root workspace.

## Deploy on Vercel

1. Import the GitHub repository in Vercel.
2. Set **Root Directory** to `apps/web`.
3. Framework preset: Next.js (defaults are fine).

### Environment variables (optional)

| Variable | Default | Purpose |
|----------|---------|---------|
| `GITHUB_REPO` | `ahmedpasic19/dumpclean` | `owner/repo` for the Releases API |
| `GITHUB_TOKEN` | (none) | GitHub PAT if you hit unauthenticated API rate limits |

Release asset filenames must match the [release workflow](../../.github/workflows/release.yml) (e.g. `dumpclean-windows-x86_64.exe`) so downloads appear on the page.

## Build

```bash
npm run build   # from repo root
```
