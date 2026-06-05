#!/usr/bin/env bash
# Patches the nightly-2026-02-01 sysroot so libstd compiles for
# riscv32imac-unknown-nuttx-elf under -Zbuild-std. That nightly's
# libstd calls sysconf(libc::_SC_HOST_NAME_MAX) for the nuttx target
# but the libc it pins lacks the constant; `[patch.crates-io] libc`
# can't fix this because -Zbuild-std resolves the std workspace
# separately. Remove this once libstd's pinned libc catches up.
#
# Anchored string replacement (not a context diff) so it survives the
# minor source differences between the toolchain's host variants.
set -euo pipefail

TOOLCHAIN="${1:-nightly-2026-02-01}"

SYSROOT=$(rustc "+${TOOLCHAIN}" --print sysroot)
TARGET_FILE="${SYSROOT}/lib/rustlib/src/rust/library/std/src/sys/net/hostname/unix.rs"

if [[ ! -f "${TARGET_FILE}" ]]; then
  echo "FATAL: expected libstd source missing: ${TARGET_FILE}" >&2
  echo "       toolchain layout changed; update this script." >&2
  exit 1
fi

if grep -q 'cfg(target_os = "nuttx")' "${TARGET_FILE}"; then
  echo "patch-nuttx-libstd: already applied to ${TARGET_FILE}"
  exit 0
fi

python3 - "${TARGET_FILE}" <<'PY'
import sys

path = sys.argv[1]
src = open(path).read()

anchor = "let host_name_max = match unsafe { libc::sysconf(libc::_SC_HOST_NAME_MAX) } {"
if anchor not in src:
    sys.exit(f"FATAL: anchor not found in {path}; upstream libstd changed")
src = src.replace(anchor, '#[cfg(not(target_os = "nuttx"))]\n    ' + anchor, 1)

closing = "        max => max as usize,\n    };\n"
if closing not in src:
    sys.exit(f"FATAL: match-closing block not found in {path}; upstream libstd changed")
src = src.replace(
    closing,
    closing + '    #[cfg(target_os = "nuttx")]\n    let host_name_max = 255usize;\n',
    1,
)

open(path, "w").write(src)
print(f"patch-nuttx-libstd: applied to {path}")
PY
