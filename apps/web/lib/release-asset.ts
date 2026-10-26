export type ReleaseAsset = {
  os: "windows" | "linux" | "macos" | "unknown";
  arch: "x86_64" | "aarch64" | "unknown";
  url: string;
  name: string;
  size: number;
  label: string;
};
