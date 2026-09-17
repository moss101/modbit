#!/usr/bin/env bash
# Build the immutable guest root image (M8.3; docs/21 "Sandbox substrate
# boundary": the guest image is immutable, versioned and contains no tenant
# secrets). The image holds `modbit-guest` as `/init`, a static BusyBox
# userland, the mount points the guest fills at boot, and nothing else.
#
# usage: tools/guest-image/build.sh <modbit-guest (static, x86_64 musl)> <busybox (static)> <out.ext4> [size_mib]
set -euo pipefail
guest=${1:?modbit-guest binary}
busybox=${2:?busybox binary}
out=${3:?output image}
size=${4:-64}
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
mkdir -p "$root"/{bin,dev,proc,sys,tmp,run,workspace,etc,usr/bin,lib}
install -m 0755 "$guest" "$root/init"
install -m 0755 "$busybox" "$root/bin/busybox"
# The applets a task's processes reach for (the same names under /usr/bin,
# where hosts keep them): the shell, the file tools, and — for egress
# through the sandbox's proxy (M8.6) — wget and nc.
for a in sh echo cat ls uname sleep yes head tail id env mkdir printf true false ps ln cp mv rm grep wc touch date hostname wget nc sed awk tr cut sort which basename dirname find xargs ip ifconfig; do
  ln -s busybox "$root/bin/$a"
  ln -s ../../bin/busybox "$root/usr/bin/$a"
done
echo modbit-guest > "$root/etc/hostname"
printf 'root:x:0:0:root:/:/bin/sh\n' > "$root/etc/passwd"
printf 'root:x:0:\n' > "$root/etc/group"
# The console and null devices the kernel hands init (device nodes need
# CAP_MKNOD; without it the image relies on the kernel mounting devtmpfs).
if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  sudo mknod -m 0600 "$root/dev/console" c 5 1
  sudo mknod -m 0666 "$root/dev/null" c 1 3
  sudo chown "$(id -u):$(id -g)" "$root/dev/console" "$root/dev/null"
fi
rm -f "$out"
truncate -s "${size}M" "$out"
mkfs.ext4 -F -q -L modbit-guest -d "$root" "$out"
echo "guest image: $out ($(stat -c %s "$out" 2>/dev/null || stat -f %z "$out") bytes) sha256=$(sha256sum "$out" | cut -d' ' -f1)"
