#!/usr/bin/env python3
"""Map Privado's privado.json to neutral flow records (PROTOCOL.md §8).

    python3 benchmark/privado/map.py <privado.json> <app name> > benchmark/results/privado/<app>.jsonl

One JSON line per dataFlow path: the hops (path relative to the scanned directory, line,
column), the source's categories through crosswalk.json, the sink class, and Privado's own ids.
Storages are not sinks for either tool and are dropped; the counts of everything dropped or
unmapped go to stderr and into the last line (`{"summary": ...}`).
"""
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent

# Privado SDK ids that are LLM providers (ThirdParties.SDK.<Vendor>...).
LLM_VENDORS = ("openai", "anthropic", "langchain", "cohere", "mistral", "huggingface", "gemini",
               "vertexai", "googlegenerativeai", "bedrock", "llamaindex", "ollama", "groq")
CONTAINER_ROOTS = ("/app/code/", "app/code/")


def rel(path):
    for prefix in CONTAINER_ROOTS:
        if path.startswith(prefix):
            return path[len(prefix):]
    return path


def sink_class(flow_type, sink_id):
    sid = (sink_id or "").lower()
    if flow_type == "leakages":
        return "log" if ".log" in sid else "other"
    if flow_type == "third_parties":
        if sid.startswith("thirdparties.sdk."):
            vendor = sid.split(".")[2] if len(sid.split(".")) > 2 else ""
            return "llm" if any(v in vendor for v in LLM_VENDORS) else "third_party"
        return "http"
    if flow_type == "internal_apis":
        return "http"
    return "other"


def main():
    doc = json.loads(pathlib.Path(sys.argv[1]).read_text())
    app = sys.argv[2]
    crosswalk = json.loads((HERE / "crosswalk.json").read_text())["map"]
    sources = {s["id"]: s for s in doc.get("sources", [])}
    counts = {"paths": 0, "dropped_storages": 0, "unmapped_elements": {}, "by_class": {}}
    rows = []
    for flow_type, entries in sorted(doc.get("dataFlow", {}).items()):
        for entry in entries:
            source_id = entry.get("sourceId")
            if flow_type == "storages":
                counts["dropped_storages"] += sum(len(s.get("paths", [])) for s in entry.get("sinks", []))
                continue
            if source_id in crosswalk:
                category = crosswalk[source_id]["fideslang"]
            else:
                category = "user"
                counts["unmapped_elements"][source_id] = counts["unmapped_elements"].get(source_id, 0) + 1
            for sink in entry.get("sinks", []):
                cls = sink_class(flow_type, sink.get("id"))
                for p in sink.get("paths", []):
                    hops = [
                        {"path": rel(h.get("fileName", "")), "line": h.get("lineNumber"), "column": h.get("columnNumber")}
                        for h in p.get("path", [])
                        if h  # the exporter's path elements are optional
                    ]
                    if not hops:
                        continue
                    counts["paths"] += 1
                    counts["by_class"][cls] = counts["by_class"].get(cls, 0) + 1
                    rows.append({
                        "tool": "privado",
                        "app": app,
                        "native_id": f"{source_id}|{sink.get('id')}|{p.get('pathId')}",
                        "flow_type": flow_type,
                        "source_element": source_id,
                        "source_name": sources.get(source_id, {}).get("name"),
                        "categories": [category],
                        "sink_class": cls,
                        "sink_id": sink.get("id"),
                        "hops": hops,
                    })
    rows.sort(key=lambda r: r["native_id"])
    for r in rows:
        print(json.dumps(r, sort_keys=True))
    print(json.dumps({"summary": counts}, sort_keys=True))
    print(f"{app}: {counts['paths']} paths, {counts['dropped_storages']} storage paths dropped, "
          f"{len(counts['unmapped_elements'])} unmapped elements", file=sys.stderr)


if __name__ == "__main__":
    main()
