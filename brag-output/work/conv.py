import json,re
d=json.load(open("ansi.json"))
SEP={"",""}
def parse(s):
    runs=[];fg=bg=None;bold=False
    for m in re.finditer(r"\x1b\[([0-9;]*)m|([^\x1b]+)",s):
        if m.group(1) is not None:
            p=[int(x) for x in m.group(1).split(";") if x]
            if p==[0] or not p: fg=bg=None;bold=False;continue
            i=0
            while i<len(p):
                if p[i]==38: fg="#%02x%02x%02x"%tuple(p[i+2:i+5]);i+=5
                elif p[i]==48: bg="#%02x%02x%02x"%tuple(p[i+2:i+5]);i+=5
                elif p[i]==1: bold=True;i+=1
                else: i+=1
        else: runs.append(dict(t=m.group(2),fg=fg,bg=bg,b=bold))
    seg=-1;prevcap=False
    for r in runs:
        if r["t"] in SEP:
            if r["t"]=="": seg+=1;prevcap=True
            r["seg"]=seg
        else:
            if not prevcap: seg+=1
            prevcap=False;r["seg"]=seg
    return runs
out={}
for k,v in d.items():
    out[k]=[parse(json.loads(l)["content"]) for l in v.strip().split("\n")] if k=="sub" else [parse(l) for l in v.rstrip("\n").split("\n")]
open("bars.js","w").write("window.BARS="+json.dumps(out,ensure_ascii=False)+";\nwindow.HOOK="+json.dumps(json.load(open("hook.txt"))["hookSpecificOutput"]["additionalContext"],ensure_ascii=False)+";\n")
