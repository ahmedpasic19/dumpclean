import { Suspense } from "react";
import { DownloadBlock } from "@/components/DownloadBlock";
import { githubRepo } from "@/lib/releases";

const REPO = githubRepo();
const REPO_URL = `https://github.com/${REPO}`;

function DownloadFallback() {
  return (
    <div className="animate-pulse rounded-lg border border-zinc-800 bg-zinc-900/50 p-5 text-sm text-zinc-500">
      Loading latest release…
    </div>
  );
}

export default function Home() {
  return (
    <main className="mx-auto flex w-full max-w-2xl flex-1 flex-col px-6 py-16">
      <header className="mb-12">
        <p className="mb-2 text-sm font-medium uppercase tracking-wider text-emerald-500">MySQL dump tool</p>
        <h1 className="text-4xl font-semibold tracking-tight text-zinc-50">dumpclean</h1>
        <p className="mt-4 text-lg leading-relaxed text-zinc-400">
          Remove tables, views, functions, procedures, and events from huge single-file MySQL dumps — without
          loading the whole file into memory. Browse in a terminal UI or automate with flags.
        </p>
      </header>

      <section className="mb-12">
        <h2 className="mb-4 text-sm font-semibold uppercase tracking-wider text-zinc-500">Features</h2>
        <ul className="list-inside list-disc space-y-2 text-zinc-300">
          <li>Stream-indexed scan of multi-gigabyte <code className="text-zinc-400">.sql</code> dumps</li>
          <li>Interactive TUI: filter, sort, mark items, write cleaned output</li>
          <li>Optional test import against MySQL before you commit changes</li>
          <li>Non-interactive <code className="text-zinc-400">--list</code>, <code className="text-zinc-400">--remove</code>, and <code className="text-zinc-400">--in-place</code></li>
        </ul>
      </section>

      <section className="mb-12">
        <h2 className="mb-4 text-sm font-semibold uppercase tracking-wider text-zinc-500">Download</h2>
        <Suspense fallback={<DownloadFallback />}>
          <DownloadBlock />
        </Suspense>
      </section>

      <section className="mb-12">
        <h2 className="mb-4 text-sm font-semibold uppercase tracking-wider text-zinc-500">Quick start</h2>
        <pre className="overflow-x-auto rounded-lg border border-zinc-800 bg-zinc-950 p-4 font-mono text-sm text-zinc-300">
          dumpclean dump.sql{"\n"}dumpclean dump.sql --remove old_table,big_view --out cleaned.sql
        </pre>
        <p className="mt-3 text-sm text-zinc-500">
          Full docs:{" "}
          <a
            href={`${REPO_URL}/blob/main/crates/dumpclean/README.md`}
            className="text-zinc-400 underline-offset-2 hover:text-zinc-300 hover:underline"
          >
            CLI README
          </a>{" "}
          and <code className="text-zinc-500">dumpclean --help</code>.
        </p>
      </section>

      <footer className="mt-auto border-t border-zinc-800 pt-8 text-sm text-zinc-500">
        <a href={REPO_URL} className="text-zinc-400 hover:text-zinc-300">
          {REPO}
        </a>
      </footer>
    </main>
  );
}
