#!/usr/bin/env python3
"""Enumerate candidate sink call sites by syntax alone, for measuring recall (PROTOCOL.md §4).

    python3 benchmark/candidates.py [--cache .benchmark-cache] [name ...]

Writes <cache>/candidates/<name>.jsonl: one line per call whose callee looks like logging,
outbound HTTP, messaging, analytics, error tracking, LLM, payment or storage API, by name
patterns written for this purpose. It does not read piiflow's catalogue or run piiflow. The
only thing shared with piiflow is which files are in scope: the same default exclusions (read
from src/files.rs so the two cannot drift) plus the corpus's own excludes, so that both tools
are measured over the same files.

The patterns over-match on purpose (a `toast.error()` is listed as a log candidate); reviewers
record `is_sink: no` for those, and they leave the recall denominator.
"""
import argparse
import json
import pathlib
import re
import subprocess

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent

EXTENSIONS = {
    "typescript": {".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"},
    "python": {".py"},
}

LOG = {"log", "debug", "info", "warn", "warning", "error", "exception", "critical", "fatal",
       "trace", "verbose", "silly", "notice"}
ERROR_TRACKING = {"captureException", "captureMessage", "captureEvent", "capture_exception",
                  "capture_message", "capture_event", "setUser", "set_user", "setContext",
                  "set_context", "setExtra", "set_extra", "setExtras", "setTag", "set_tag",
                  "setTags", "addBreadcrumb", "add_breadcrumb"}
ANALYTICS = {"track", "identify", "capture", "alias", "group", "page", "screen", "groupIdentify",
             "group_identify", "register", "people_set"}
MESSAGING = {"send", "send_mail", "sendmail", "send_mass_mail", "sendMail", "send_email",
             "sendEmail", "send_message", "sendMessage", "send_messages", "postMessage",
             "chat_postMessage", "deliver", "mail_admins", "mail_managers", "sendSms", "send_sms",
             "notify", "publish"}
LLM_METHODS = {"generateText", "streamText", "generateObject", "streamObject", "embed", "embedMany",
               "invoke", "ainvoke", "stream", "astream", "complete", "acomplete", "chat", "achat",
               "stream_chat", "astream_chat", "stream_complete", "embed_query", "embed_documents",
               "aembed_query", "get_text_embedding", "get_query_embedding", "predict", "apredict",
               "query", "aquery", "generate", "agenerate", "create", "parse"}
LLM_RECEIVER = re.compile(r"(completions|chat|messages|responses|embeddings|llm|model|openai|"
                          r"anthropic|gemini|mistral|cohere|ollama|chain|agent|engine|embed\w*)$",
                          re.I)
THIRD_PARTY_CREATE = re.compile(r"(customers|checkout|sessions|charges|paymentIntents|"
                                r"payment_intents|subscriptions|invoices|emails|calls|contacts|"
                                r"events|messages|webhooks|transfers|refunds)$", re.I)
HTTP_METHODS = {"get", "post", "put", "patch", "delete", "request", "head", "options", "stream",
                "send", "fetch", "aget", "apost", "aput", "apatch", "adelete", "arequest"}
HTTP_RECEIVER = re.compile(r"(requests|httpx|axios|client|session|http|https|api|fetcher|got|ky|"
                           r"superagent|urllib3?|aiohttp|pool|transport|AsyncClient|Client)$", re.I)
HTTP_BARE = {"fetch", "urlopen", "ofetch", "$fetch", "request", "got", "ky", "needle"}
STORAGE = {"setItem", "put_object", "putObject", "upload_file", "upload_fileobj", "upload",
           "PutObjectCommand", "Upload", "set_cookie", "setCookie", "set"}
STORAGE_RECEIVER = re.compile(r"(localStorage|sessionStorage|s3|bucket|blob|storage|Cookies|"
                              r"cookies|cookieStore|minio|gcs|container_client)$", re.I)
# The application's own response objects are not sinks.
OWN_RESPONSE = re.compile(r"^(res|response|reply|ctx|resp|socket|ws|io|emitter|events?|bus|"
                          r"subject|queue|channel|router|app)$")

CALL = re.compile(r"(?<![\w$.])(new\s+)?((?:[A-Za-z_$][\w$]*\??\.)*[A-Za-z_$][\w$]*)\s*\(")
DEFINITION = re.compile(r"^\s*(def|async def|function|class)\s|^\s*(public|private|protected|static|async)?\s*[A-Za-z_$][\w$]*\s*\([^)]*\)\s*(:\s*[\w<>\[\], |.]+)?\s*\{\s*$")
COMMENT = re.compile(r"^\s*(#|//|\*|/\*)")


def classify(chain, new):
    parts = [p.rstrip("?") for p in chain.split(".")]
    method, receiver = parts[-1], parts[:-1]
    last = receiver[-1] if receiver else ""
    if last and OWN_RESPONSE.match(last):
        return None
    if new and method == "PutObjectCommand":
        return "storage"
    if not receiver:
        if method == "print":
            return "log"
        if method in HTTP_BARE:
            return "http"
        if method in {"generateText", "streamText", "generateObject", "streamObject", "embed", "embedMany"}:
            return "llm"
        if method in {"send_mail", "send_mass_mail", "mail_admins", "mail_managers", "sendmail"}:
            return "messaging"
        return None
    chain_lower = ".".join(receiver)
    if method in ERROR_TRACKING:
        return "error_tracking"
    if method == "create" and THIRD_PARTY_CREATE.search(last):
        return "third_party"
    if method in LLM_METHODS and (LLM_RECEIVER.search(last) or re.search(r"openai|anthropic|llm|langchain", chain_lower, re.I)):
        return "llm"
    if method in HTTP_METHODS and HTTP_RECEIVER.search(last):
        return "http"
    if method in STORAGE and STORAGE_RECEIVER.search(last):
        return "storage"
    if method in LOG:
        return "log"
    if method in ANALYTICS and method not in {"page", "group", "register", "alias"} or (method in {"page", "group", "register", "alias"} and re.search(r"analytics|posthog|segment|mixpanel|amplitude", chain_lower, re.I)):
        return "analytics"
    if method in MESSAGING:
        return "messaging"
    return None


def default_excludes():
    src = (ROOT / "src/files.rs").read_text()
    block = src[src.index("pub const DEFAULT_EXCLUDES"):]
    block = block[: block.index("];")]
    return re.findall(r'"([^"]+)"', block)


def glob_regex(glob):
    out, i = "", 0
    while i < len(glob):
        if glob.startswith("**/", i):
            out += r"(?:.*/)?"
            i += 3
        elif glob.startswith("**", i):
            out += r".*"
            i += 2
        elif glob[i] == "*":
            out += r"[^/]*"
            i += 1
        else:
            out += re.escape(glob[i])
            i += 1
    return re.compile(out + r"\Z")


def files_in_scope(root, language, excludes):
    listed = subprocess.run(["git", "ls-files", "-z"], cwd=root, capture_output=True, check=True).stdout
    exts = EXTENSIONS[language]
    for rel in sorted(listed.decode().split("\0")):
        if rel and pathlib.PurePosixPath(rel).suffix in exts and not any(g.match(rel) for g in excludes):
            yield rel


def enumerate_sites(root, language, excludes):
    for rel in files_in_scope(root, language, excludes):
        try:
            lines = (root / rel).read_text(encoding="utf-8").splitlines()
        except (UnicodeDecodeError, OSError):
            continue
        for n, text in enumerate(lines, 1):
            if COMMENT.match(text) or DEFINITION.match(text) or len(text) > 400:
                continue
            for m in CALL.finditer(text):
                cls = classify(m.group(2), bool(m.group(1)))
                if cls:
                    yield {"path": rel, "line": n, "column": m.start(2) + 1, "callee": m.group(2), "class": cls}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default=str(ROOT / ".benchmark-cache"))
    ap.add_argument("names", nargs="*")
    args = ap.parse_args()
    corpus = json.loads((HERE / "corpus.json").read_text())
    cache = pathlib.Path(args.cache)
    base = [glob_regex(g) for g in default_excludes()]
    (cache / "candidates").mkdir(parents=True, exist_ok=True)
    for entry in corpus["repositories"]:
        if args.names and entry["name"] not in args.names:
            continue
        root = cache / entry["name"] / entry["scope"]
        excludes = base + [glob_regex(g) for g in entry["exclude"]]
        sites = list(enumerate_sites(root, entry["language"], excludes))
        with open(cache / "candidates" / f"{entry['name']}.jsonl", "w") as out:
            for s in sites:
                out.write(json.dumps(s, sort_keys=True) + "\n")
        by = {}
        for s in sites:
            by[s["class"]] = by.get(s["class"], 0) + 1
        print(f"{entry['name']}: {len(sites)} candidate sites {dict(sorted(by.items()))}")


if __name__ == "__main__":
    main()
