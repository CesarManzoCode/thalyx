"""A language server that answers some questions and withholds others.

Rule 8: a fake must model the property under test, and the property here is
**that one request of a query can fail while another succeeds**. A stand-in
that answered everything could not express it, and a real rust-analyzer can
only be made to express it by being cold, which is not a state a test can ask
for.

It speaks enough LSP to be driven by `thalyx_rust::analyzer`: the framing, the
`initialize` handshake, the end of cache priming that the client waits for, and
three requests. What it does with the last two is the argument:

    whole           every question answered — the control, so that an absent
                    datum below is the withholding and not a broken stand-in
    no-hover        `textDocument/hover` is never answered at all, which is
                    what a cold rust-analyzer did on a fresh Thalyx VM on
                    2026-09-05 and is why this file exists
    no-references   `textDocument/references` comes back as an error

Usage: server.py <workspace root> <mode>
"""

import json
import sys

root = sys.argv[1]
mode = sys.argv[2]

# Where the stand-in says the one `Flywheel` is: `pub struct Flywheel;`, the
# first line of hub.rs, with the name at character 11. Zero-based, as the wire
# is.
DECLARATION = ("src/hub.rs", 0, 11)
USES = [("src/rim.rs", 2, 15), ("src/spoke.rs", 2, 15)]


def uri(relative):
    return "file://" + root + "/" + relative


def place(where):
    path, line, character = where
    return {
        "uri": uri(path),
        "range": {
            "start": {"line": line, "character": character},
            "end": {"line": line, "character": character + len("Flywheel")},
        },
    }


def read():
    length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.strip()
        if not line:
            break
        if line.lower().startswith(b"content-length:"):
            length = int(line.split(b":")[1])
    if length is None:
        return None
    return json.loads(sys.stdin.buffer.read(length))


def write(message):
    body = json.dumps(message).encode()
    sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n" % len(body))
    sys.stdout.buffer.write(body)
    sys.stdout.buffer.flush()


def answer(request, result):
    write({"jsonrpc": "2.0", "id": request["id"], "result": result})


while True:
    request = read()
    if request is None:
        break
    method = request.get("method")
    # A notification. `textDocument/didOpen` is one, and a server that answered
    # it would be answering with an id nobody is waiting for.
    if "id" not in request:
        if method == "exit":
            break
        continue

    if method == "initialize":
        answer(request, {"capabilities": {"positionEncoding": "utf-8"}})
        # The client waits for this before asking anything; without it every
        # question would be measuring READY_CEILING instead of the server.
        write({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": "rustAnalyzer/cachePriming",
                "value": {"kind": "end"},
            },
        })
    elif method == "workspace/symbol":
        # Exactly one declaration of the name asked about: this is the request
        # that resolves a name, and in every mode below it succeeds.
        answer(request, [{
            "name": request["params"]["query"],
            "kind": 23,
            "location": place(DECLARATION),
        }])
    elif method == "textDocument/hover":
        if mode != "no-hover":
            answer(request, {"contents": {
                "kind": "markdown",
                "value": "```rust\npub struct Flywheel\n```",
            }})
    elif method == "textDocument/references":
        if mode == "no-references":
            write({"jsonrpc": "2.0", "id": request["id"],
                   "error": {"code": -32603, "message": "not today"}})
        else:
            answer(request, [place(use_site) for use_site in USES])
    elif method == "shutdown":
        answer(request, None)
    else:
        answer(request, None)
