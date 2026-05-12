"""mitmproxy addon: capture WebSocket + Anthropic HTTP traffic."""
import os
import time
from mitmproxy import http

OUT_DIR = os.path.dirname(os.path.abspath(__file__))
conn_id = 0


def _ts() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime())


# ── WebSocket ─────────────────────────────────────────────

def websocket_start(flow):
    global conn_id
    conn_id += 1
    ts = _ts()
    url = flow.request.url
    fn = os.path.join(OUT_DIR, f"ws_{ts}.txt")
    flow.metadata["ws_file"] = fn
    os.makedirs(OUT_DIR, exist_ok=True)
    with open(fn, "w") as f:
        f.write(f"# URL: {url}\n# Started: {ts}\n\n")
    print(f"[WS START] {url}")


def websocket_message(flow):
    msg = flow.websocket.messages[-1]
    direction = "sent" if msg.from_client else "received"
    fn = flow.metadata.get("ws_file")
    if not fn:
        return
    data = bytes(msg.content).decode("utf-8", errors="replace")
    with open(fn, "a") as f:
        f.write(f"{direction}:\n{data}\n\n")
    print(f"[WS] {direction} ({len(msg.content)}b)")


def websocket_end(flow):
    fn = flow.metadata.get("ws_file")
    if not fn:
        return
    code = flow.websocket.close_code
    with open(fn, "a") as f:
        f.write(f"# Closed: code={code}\n")
    print(f"[WS END] code={code}")


# ── HTTP (Anthropic API) ──────────────────────────────────

def request(flow: http.HTTPFlow):
    """Capture outbound requests to Anthropic API."""
    url = flow.request.pretty_url
    if "anthropic.com" not in url:
        return
    ts = _ts()
    global conn_id
    conn_id += 1
    fn = os.path.join(OUT_DIR, f"http_{ts}.txt")
    flow.metadata["http_file"] = fn
    os.makedirs(OUT_DIR, exist_ok=True)
    with open(fn, "w") as f:
        f.write(f"# {flow.request.method} {url}\n# Time: {ts}\n\n")
        headers = dict(flow.request.headers)
        f.write(f">>> REQUEST >>>\n")
        f.write(f"{flow.request.method} {url}\n")
        for k, v in headers.items():
            # Mask sensitive headers
            if k.lower() in ("x-api-key", "authorization", "api-key"):
                v = v[:8] + "..." if len(v) > 8 else "..."
            f.write(f"{k}: {v}\n")
        f.write(f"\n")
        body = flow.request.get_text()
        if body:
            f.write(body)
        f.write(f"\n\n")
    print(f"[HTTP REQ] {flow.request.method} {url}")


def response(flow: http.HTTPFlow):
    """Capture responses from Anthropic API."""
    url = flow.request.pretty_url
    if "anthropic.com" not in url:
        return
    fn = flow.metadata.get("http_file")
    if not fn:
        return
    with open(fn, "a") as f:
        f.write(f"<<< RESPONSE <<<\n")
        f.write(f"Status: {flow.response.status_code}\n")
        headers = dict(flow.response.headers)
        for k, v in headers.items():
            f.write(f"{k}: {v}\n")
        f.write(f"\n")
        body = flow.response.get_text()
        if body:
            f.write(body)
    print(f"[HTTP RESP] {flow.response.status_code} {url}")
