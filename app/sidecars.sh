#!/bin/sh
# Builds what the app carries inside it: the revui binary, the page, and the page's server as one executable.
set -eu
cd "$(dirname "$0")"
target="${TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
(cd .. && cargo build --release --locked --target "$target")
(cd ../web && pnpm install --frozen-lockfile && pnpm build)
mkdir -p src-tauri/binaries
cp "../target/$target/release/revui" "src-tauri/binaries/revui-$target"
case "$target" in aarch64-*) bun_target=bun-darwin-arm64 ;; *) bun_target=bun-darwin-x64 ;; esac
pnpm exec bun build --compile --target "$bun_target" --external vite --define 'process.env.NODE_ENV="production"' ../web/server/main.ts --outfile "src-tauri/binaries/revui-server-$target"
