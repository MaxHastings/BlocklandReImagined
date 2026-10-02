#!/usr/bin/env python3
"""Move line ranges of a method into new methods of the same impl, unchanged.

Usage: extract.py <file> <spec.json>; spec is a list of
{"start", "end", "name", "params", "args", "doc", "returns_result"},
optionally "ret", "call" and "tail" (the method's last line) for a value.
Ranges are 1-based, inclusive, and are processed bottom-up. Bodies are
de-indented by `dedent` spaces (the method's body indent minus 8).
"""
import json, sys
path, spec = sys.argv[1], json.load(open(sys.argv[2]))
lines = open(path).read().split("\n")
# The impl's closing brace: the last line that is exactly "}".
end_impl = max(i for i, l in enumerate(lines) if l == "}")
new = []
for s in sorted(spec, key=lambda s: -s["start"]):
    body = lines[s["start"] - 1 : s["end"]]
    d = s.get("dedent", 0)
    body = [l[d:] if l.strip() else l for l in body]
    for old, rep in s.get("replace", []):
        body = [l.replace(old, rep) for l in body]
    ret = s.get("ret", " -> Result<()>" if s["returns_result"] else "")
    call = s.get("call", f"self.{s['name']}({s['args']})" + ("?;" if s["returns_result"] else ";"))
    lines[s["start"] - 1 : s["end"]] = [" " * (8 + 0) + call]
    method = [""] + ["    /// " + l for l in s["doc"]] + [f"    fn {s['name']}(&mut self{s['params']}){ret} {{"] + body
    if "tail" in s:
        method.append("        " + s["tail"])
    elif s["returns_result"]:
        method.append("        Ok(())")
    method.append("    }")
    new = method + new
end_impl = max(i for i, l in enumerate(lines) if l == "}")
lines[end_impl:end_impl] = new
open(path, "w").write("\n".join(lines))
