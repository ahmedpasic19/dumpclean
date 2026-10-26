import { DownloadSection } from "@/components/DownloadSection";
import { fetchLatestRelease, githubRepo } from "@/lib/releases";

const REPO_URL = `https://github.com/${githubRepo()}`;

export async function DownloadBlock() {
  let release = null;
  let releaseError: string | null = null;
  try {
    release = await fetchLatestRelease();
  } catch {
    releaseError =
      "Could not load release info from GitHub. Try again later or download from GitHub directly.";
  }

  if (releaseError) {
    return <p className="mb-4 text-sm text-amber-400/90">{releaseError}</p>;
  }

  if (release && release.assets.length > 0) {
    return (
      <DownloadSection assets={release.assets} releaseUrl={release.htmlUrl} tagName={release.tagName} />
    );
  }

  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-5 text-zinc-400">
      <p className="font-medium text-zinc-200">Releases coming soon</p>
      <p className="mt-2 text-sm">
        Pre-built binaries will appear here after the first tagged release. Build from source:
      </p>
      <pre className="mt-3 overflow-x-auto rounded bg-zinc-950 p-3 font-mono text-sm text-zinc-300">
        cd crates/dumpclean{"\n"}cargo build --release
      </pre>
      <p className="mt-3 text-sm">
        <a href={`${REPO_URL}/releases`} className="text-emerald-400 hover:text-emerald-300">
          Check GitHub Releases
        </a>
      </p>
    </div>
  );
}
