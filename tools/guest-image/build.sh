#!/usr/bin/env bash
# Build the immutable guest root image (M8.3; docs/21 "Sandbox substrate
# boundary": the guest image is immutable, versioned and contains no tenant
# secrets). The image holds `modbit-guest` as `/init`, a static BusyBox
# userland, the mount points the guest fills at boot, and nothing else.
#
# usage: tools/guest-image/build.sh <modbit-guest (static, x86_64 musl)> <busybox (static)> <out.ext4> [size_mib] [chromium_dir]
#
# With a fifth argument — a Chrome for Testing `chrome-headless-shell-linux64`
# directory — the image also carries the guest's browser (M8.8, docs/22
# "Cloud browser"): the tree at /opt/chromium, every shared library it links
# (the closure `ldd` resolves on the build host, glibc and its loader
# included), NSS's loadable modules, one font and a fontconfig for it. The
# browser has no network of its own inside the VM: it is pointed at the
# guest's egress proxy like any process.
set -euo pipefail
guest=${1:?modbit-guest binary}
busybox=${2:?busybox binary}
out=${3:?output image}
size=${4:-64}
chromium=${5:-}
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
if [ -n "$chromium" ]; then
  test -x "$chromium/chrome-headless-shell" || { echo "no chrome-headless-shell in $chromium" >&2; exit 2; }
  mkdir -p "$root/opt" "$root/lib/x86_64-linux-gnu" "$root/lib64" "$root/usr/share/fonts" "$root/etc/fonts"
  cp -a "$chromium" "$root/opt/chromium"
  # The shared-library closure of every ELF in the tree, plus NSS's
  # dlopen()ed modules (the certificate and key stores Chromium loads
  # by name), resolved on this host and laid out where the loader looks.
  copy_libs() {
    ldd "$1" 2>/dev/null | awk '/=> \//{print $3} /^\t\//{print $1}' | while read -r lib; do
      [ -f "$lib" ] || continue
      case "$lib" in
        */ld-linux-x86-64.so.2) install -m 0755 "$lib" "$root/lib64/ld-linux-x86-64.so.2" ;;
        *) [ -f "$root/lib/x86_64-linux-gnu/$(basename "$lib")" ] || install -m 0755 "$lib" "$root/lib/x86_64-linux-gnu/" ;;
      esac
    done
  }
  copy_libs "$chromium/chrome-headless-shell"
  for so in "$chromium"/*.so "$chromium"/*.so.*; do [ -f "$so" ] && copy_libs "$so"; done
  for nssdir in /usr/lib/x86_64-linux-gnu/nss /usr/lib/x86_64-linux-gnu; do
    for m in libsoftokn3.so libfreeblpriv3.so libfreebl3.so libnssckbi.so libnssdbm3.so; do
      if [ -f "$nssdir/$m" ]; then
        install -m 0755 "$nssdir/$m" "$root/lib/x86_64-linux-gnu/$m"
        copy_libs "$nssdir/$m"
      fi
    done
  done
  # One font family so pages render text; fontconfig finds it here.
  for f in /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf /usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf /usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf; do
    [ -f "$f" ] && install -m 0644 "$f" "$root/usr/share/fonts/"
  done
  cat > "$root/etc/fonts/fonts.conf" <<'FC'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig>
  <dir>/usr/share/fonts</dir>
  <cachedir>/tmp/fontconfig</cachedir>
  <alias><family>sans-serif</family><prefer><family>DejaVu Sans</family><family>Liberation Sans</family></prefer></alias>
  <alias><family>serif</family><prefer><family>DejaVu Sans</family></prefer></alias>
  <alias><family>monospace</family><prefer><family>DejaVu Sans</family></prefer></alias>
</fontconfig>
FC
  printf 'root:x:0:0:root:/:/bin/sh\nnobody:x:65534:65534:nobody:/:/bin/false\n' > "$root/etc/passwd"
  printf 'hosts: files\npasswd: files\ngroup: files\n' > "$root/etc/nsswitch.conf"
  printf '127.0.0.1 localhost\n' > "$root/etc/hosts"
  echo "chromium: $(ls "$root/lib/x86_64-linux-gnu" | wc -l) libraries copied for $chromium"
fi
rm -f "$out"
truncate -s "${size}M" "$out"
mkfs.ext4 -F -q -L modbit-guest -d "$root" "$out"
echo "guest image: $out ($(stat -c %s "$out" 2>/dev/null || stat -f %z "$out") bytes) sha256=$(sha256sum "$out" | cut -d' ' -f1)"
