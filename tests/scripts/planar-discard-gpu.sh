#!/usr/bin/env bash
set -euo pipefail

mode="${1:-patched}"
if [[ "$mode" != "baseline" && "$mode" != "patched" ]]; then
    echo "usage: $0 baseline|patched" >&2
    exit 64
fi

mkdir -p /workspace/tmp
export TMPDIR=/workspace/tmp
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"
export CARGO_TARGET_DIR="/workspace/wgpu-10711-target-${mode}"
export CXX=g++ CC=gcc
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=g++

if ! command -v cargo >/dev/null 2>&1; then
    export RUSTUP_HOME=/workspace/rustup
    export CARGO_HOME=/workspace/cargo
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --default-toolchain stable
    export PATH="$CARGO_HOME/bin:$PATH"
fi

base_commit=f48207305932460fe1c2f01b75944720555823ed
if [[ "$mode" == "baseline" ]]; then
    root="$(git rev-parse --show-toplevel)"
    git -C "$root" fetch --no-tags origin "$base_commit"
    patch="/workspace/tmp/planar-discard-test-${BASHPID}.patch"
    git -C "$root" diff --binary "$base_commit" HEAD -- tests/tests/wgpu-gpu/planar_texture/mod.rs > "$patch"
    test -s "$patch"
    worktree="/workspace/tmp/wgpu-baseline-${BASHPID}"
    git -C "$root" worktree add --detach "$worktree" "$base_commit"
    git -C "$worktree" apply "$patch"
    cd "$worktree"
fi

printf 'RUN_MODE=%s SHA=%s\n' "$mode" "$(git rev-parse HEAD)"
echo 'GPU HARDWARE'
command -v nvidia-smi && nvidia-smi -L || true
command -v vulkaninfo && vulkaninfo --summary | head -60 || true
echo 'COMPILER'
rustc --version
cargo --version
df -h /workspace | tail -1

echo 'GPU ENUMERATION'
WGPU_INFO_SAVE_GPUCONFIG_REPORT=1 cargo test -p wgpu-info --bin wgpu-info generate_gpuconfig_report -- --nocapture
python3 - <<'PY'
import json
from pathlib import Path
report = json.loads(Path('.gpuconfig').read_text())
for i, device in enumerate(report['devices']):
    print('DEVICE', i, device['info']['name'], device['info']['backend'], 'NV12', 'TEXTURE_FORMAT_NV12' in str(device['features']))
if not report['devices']:
    raise SystemExit('ERROR: no GPU device was enumerated')
PY

echo 'PLANAR DISCARD REAL GPU READBACK TEST'
export WGPU_BACKEND=vulkan
unset WGPU_GPU_TESTS_USE_NOOP_BACKEND || true
cargo test -p wgpu-test --test wgpu-gpu nv12_plane_discard_preserves_other_plane -- --nocapture
printf 'SUCCESS_MODE=%s\n' "$mode"
