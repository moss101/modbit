import json,sys
import os
HERE=os.path.dirname(os.path.abspath(__file__))
d=json.load(open(os.path.join(HERE,"..","..","..","evidence","zcode-ui","reference","zcode-colors-rgba.json")))
def lin(v):
    s=v/255; return s/12.92 if s<=0.04045 else ((s+0.055)/1.055)**2.4
def lum(rgb): return 0.2126*lin(rgb[0])+0.7152*lin(rgb[1])+0.0722*lin(rgb[2])
def comp(fg,bg):
    a=fg[3]; return [fg[i]*a+bg[i]*(1-a) for i in range(3)]
def cr(fg,bg):
    f=comp(fg,bg) if fg[3]<1 else fg[:3]
    a,b=lum(f),lum(bg[:3]); hi,lo=max(a,b),min(a,b); return (hi+.05)/(lo+.05)
def hexs(c): return '#%02x%02x%02x'%tuple(int(round(x)) for x in c[:3])
for theme in ("light","dark"):
    t={k[8:]:v for k,v in d[theme].items()}
    def flat(k):
        c=t[k]; return c if c[3]>=1 else comp(c, t["background"])+[1]
    print("=== ZAI", theme)
    texts=["foreground","foreground-subtle","foreground-subtlest","remote-foreground-muted","success","warning","destructive","brand","interaction-confirmation-foreground","success-surface-foreground","warning-surface-foreground","destructive-surface-foreground","progress-surface-foreground","git-modified","git-added","tooltip-foreground","tooltip-tag-foreground","interaction-ask-foreground","terminal-blue","terminal-bright-blue","idle-task","file-node-foreground","skill-node-foreground"]
    for tx in texts:
        if tx not in t: print("  (no token)",tx); continue
        row=["%s=%.2f"%(s,cr(t[tx],flat(s))) for s in ("background","panel","card","sidebar","tooltip")]
        print(" ", tx.ljust(38), " ".join(row))
    print("  border on panel %.2f | border-hover %.2f | input-border-focused %.2f"%(cr(t["border"],flat("panel")),cr(t["border-hover"],flat("panel")),cr(t["input-border-focused"],flat("panel"))))
    for fg,bg in (("primary-foreground","primary"),("destructive-foreground","destructive"),("warning-foreground","warning"),("success-foreground","success"),("diff-added-foreground","diff-added")):
        if fg in t and bg in t: print("  %s on %s: %.2f"%(fg,bg,cr(t[fg],flat(bg))))
    for tx in ("foreground","remote-foreground-muted","foreground-subtle"):
        print("  %s on hover %.2f on selected %.2f"%(tx,cr(t[tx],comp(t["hover"],t["panel"])+[1]),cr(t[tx],comp(t["selected"],t["panel"])+[1])))
    print("  secondary:",hexs(flat("secondary")),"tag",hexs(flat("tag")),"surface over panel",hexs(comp(t["surface"],t["panel"])))
