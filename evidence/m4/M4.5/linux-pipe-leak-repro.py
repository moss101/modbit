"""Simulate the desktop: an outer harness (Playwright) spawns a supervisor
(Electron) with piped stdout/stderr and waits for those pipes to close; the
supervisor spawns the Core with its own pipes but leaks a non-CLOEXEC dup of
the harness pipe into it (what a browser process does). The Core spawns the
broker, which outlives the Core. Measure how long after the Core exits the
harness sees EOF on the supervisor's stdout."""
import os, subprocess, sys, time, tempfile

core = sys.argv[1]
execd = sys.argv[2]
role = sys.argv[3] if len(sys.argv) > 3 else "harness"

if role == "harness":
    p = subprocess.Popen([sys.executable, __file__, core, execd, "supervisor"],
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    t0 = time.time()
    out = p.stdout.read()   # EOF only when every holder of the write end is gone
    p.wait()
    print(f"harness: supervisor exit={p.returncode} stdout EOF after {time.time()-t0:.1f}s; child said: {out.decode().strip()}")
    sys.exit(0)

# supervisor
data = tempfile.mkdtemp(prefix="modbit-leak-")
leaked = os.dup(1)           # a dup of the harness pipe, inheritable (no CLOEXEC)
os.set_inheritable(leaked, True)
env = dict(os.environ, MODBIT_EXECD_BIN=execd, MODBIT_EXECD_ORPHAN_GRACE_SECS="20")
c = subprocess.Popen([core, "--data-dir", data, "--tether-stdin"], stdin=subprocess.PIPE,
                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env,
                     close_fds=False, pass_fds=(leaked,))
ready = c.stdout.readline()
time.sleep(1.0)   # broker spawned by now
# which fds does the broker hold?
brokers = [pid for pid in os.listdir('/proc') if pid.isdigit() and
           os.path.exists(f'/proc/{pid}/cmdline') and b'modbit-execd' in open(f'/proc/{pid}/cmdline','rb').read()]
held = {}
for pid in brokers:
    try:
        held[pid] = sorted(os.readlink(f'/proc/{pid}/fd/{fd}') for fd in os.listdir(f'/proc/{pid}/fd'))
    except OSError as e:
        held[pid] = [str(e)]
t0 = time.time()
c.stdin.close()              # the tether: the Core exits now
c.wait()
core_exit = time.time() - t0
os.close(leaked)
sys.stdout.write(f"core exited in {core_exit:.2f}s; broker fds: {held}\n")
sys.stdout.flush()
# the supervisor exits; the harness's EOF now depends only on who still holds the pipe
