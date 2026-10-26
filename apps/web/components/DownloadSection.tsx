"use client";

import { useSyncExternalStore } from "react";
import { formatBytes } from "@/lib/formatBytes";
import type { ReleaseAsset } from "@/lib/release-asset";

type Props = {
  assets: ReleaseAsset[];
  releaseUrl: string;
  tagName: string;
};

type PlatformHint = { os: ReleaseAsset["os"]; arch: ReleaseAsset["arch"] };

function detectPlatform(): PlatformHint {
  const ua = navigator.userAgent.toLowerCase();
  const platform = (navigator.platform ?? "").toLowerCase();

  let os: ReleaseAsset["os"] = "unknown";
  if (ua.includes("win")) os = "windows";
  else if (ua.includes("mac") || platform.includes("mac")) os = "macos";
  else if (ua.includes("linux") || platform.includes("linux")) os = "linux";

  let arch: ReleaseAsset["arch"] = "unknown";
  if (ua.includes("aarch64") || ua.includes("arm64")) arch = "aarch64";
  else if (ua.includes("x86_64") || ua.includes("amd64") || ua.includes("win64")) {
    arch = "x86_64";
  } else if (os === "macos" && ua.includes("intel mac")) {
    arch = "x86_64";
  }

  return { os, arch };
}

const serverHint: PlatformHint = { os: "unknown", arch: "unknown" };

function subscribePlatform() {
  return () => {};
}

function matchesHint(asset: ReleaseAsset, hint: PlatformHint): boolean {
  if (hint.os === "unknown") return false;
  if (asset.os !== hint.os) return false;
  if (hint.arch !== "unknown" && asset.arch !== "unknown" && asset.arch !== hint.arch) {
    return false;
  }
  return true;
}

export function DownloadSection({ assets, releaseUrl, tagName }: Props) {
  const hint = useSyncExternalStore(subscribePlatform, detectPlatform, () => serverHint);

  const recommended = assets.find((a) => matchesHint(a, hint));

  return (
    <div className="space-y-4">
      {recommended && (
        <a
          href={recommended.url}
          className="inline-flex w-full items-center justify-center rounded-lg bg-emerald-600 px-5 py-3 text-sm font-semibold text-white transition hover:bg-emerald-500 sm:w-auto"
        >
          Download {recommended.label} ({tagName})
        </a>
      )}

      <ul className="divide-y divide-zinc-800 rounded-lg border border-zinc-800 bg-zinc-900/50">
        {assets.map((asset) => {
          const isRec = recommended?.name === asset.name;
          return (
            <li key={asset.name} className="flex flex-wrap items-center justify-between gap-2 px-4 py-3">
              <div>
                <span className="font-medium text-zinc-100">{asset.label}</span>
                {isRec && (
                  <span className="ml-2 rounded bg-emerald-950 px-2 py-0.5 text-xs text-emerald-400">
                    recommended
                  </span>
                )}
                <p className="text-xs text-zinc-500">{formatBytes(asset.size)}</p>
              </div>
              <a
                href={asset.url}
                className="text-sm font-medium text-emerald-400 hover:text-emerald-300"
              >
                Download
              </a>
            </li>
          );
        })}
      </ul>

      <p className="text-sm text-zinc-500">
        <a href={releaseUrl} className="text-zinc-400 underline-offset-2 hover:text-zinc-300 hover:underline">
          View all files and release notes on GitHub
        </a>
      </p>
    </div>
  );
}
