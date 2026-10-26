import type { ReleaseAsset } from "@/lib/release-asset";

export type { ReleaseAsset } from "@/lib/release-asset";

export type LatestRelease = {
  tagName: string;
  name: string;
  publishedAt: string;
  htmlUrl: string;
  body: string;
  assets: ReleaseAsset[];
};

const ASSET_PATTERNS: {
  pattern: RegExp;
  os: ReleaseAsset["os"];
  arch: ReleaseAsset["arch"];
  label: string;
}[] = [
  {
    pattern: /^dumpclean-windows-x86_64\.exe$/i,
    os: "windows",
    arch: "x86_64",
    label: "Windows (x64)",
  },
  {
    pattern: /^dumpclean-linux-x86_64$/i,
    os: "linux",
    arch: "x86_64",
    label: "Linux (x64)",
  },
  {
    pattern: /^dumpclean-macos-x86_64$/i,
    os: "macos",
    arch: "x86_64",
    label: "macOS (Intel)",
  },
  {
    pattern: /^dumpclean-macos-aarch64$/i,
    os: "macos",
    arch: "aarch64",
    label: "macOS (Apple Silicon)",
  },
];

function parseAsset(name: string, url: string, size: number): ReleaseAsset | null {
  for (const { pattern, os, arch, label } of ASSET_PATTERNS) {
    if (pattern.test(name)) {
      return { os, arch, url, name, size, label };
    }
  }
  return null;
}

type GitHubReleaseJson = {
  tag_name: string;
  name: string;
  published_at: string;
  html_url: string;
  body: string;
  assets: { name: string; browser_download_url: string; size: number }[];
};

export function githubRepo(): string {
  return process.env.GITHUB_REPO ?? "ahmedpasic19/dumpclean";
}

export async function fetchLatestRelease(): Promise<LatestRelease | null> {
  const repo = githubRepo();
  const headers: HeadersInit = {
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
  };
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }

  const res = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, {
    headers,
    next: { revalidate: 3600 },
  });

  if (res.status === 404) {
    return null;
  }
  if (!res.ok) {
    throw new Error(`GitHub API error: ${res.status}`);
  }

  const data = (await res.json()) as GitHubReleaseJson;
  const assets = data.assets
    .map((a) => parseAsset(a.name, a.browser_download_url, a.size))
    .filter((a): a is ReleaseAsset => a !== null);

  const order = ["windows", "linux", "macos"] as const;
  assets.sort((a, b) => {
    const oa = order.indexOf(a.os === "unknown" ? "linux" : a.os);
    const ob = order.indexOf(b.os === "unknown" ? "linux" : b.os);
    if (oa !== ob) return oa - ob;
    if (a.arch === "x86_64" && b.arch === "aarch64") return -1;
    if (a.arch === "aarch64" && b.arch === "x86_64") return 1;
    return a.label.localeCompare(b.label);
  });

  return {
    tagName: data.tag_name,
    name: data.name || data.tag_name,
    publishedAt: data.published_at,
    htmlUrl: data.html_url,
    body: data.body,
    assets,
  };
}
