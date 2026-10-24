"""Copies ZCode's real styles.css for the reference harness, dropping only the imports of assets the harness does not ship.

Usage: python3 -I transform_styles.py <upstream styles.css> <harness src/styles.css>
Nothing else is changed: every token, rule and @theme block is the upstream text.
"""
import re
import sys

DROP = [
    r'@import "@xterm/xterm/css/xterm.css";',
    r'@import "katex/dist/katex.min.css";',
    r'@import "./highspeed/highspeed-activation.css";',
    r'@import "./highspeed/highspeed-share.css";',
    r'@source "../../../node_modules/streamdown/dist/\*.js";.*',
]

src = open(sys.argv[1], encoding="utf8").read()
for pattern in DROP:
    src, n = re.subn(pattern, "", src)
    if n != 1:
        sys.exit("expected exactly one match for %s, found %d: the upstream stylesheet changed" % (pattern, n))
open(sys.argv[2], "w", encoding="utf8").write(src)
