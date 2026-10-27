#!/usr/bin/env python3
"""Opt-in real PDF -> MCP save -> paged read -> owner restart check.
Uses an owned disposable vault; does not touch the running user's app.
Run after building gui,codex bins: python3 examples/paper_smoke.py --url https://.../paper.pdf
"""
import argparse
import hashlib
import json
import pathlib
import select
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--url', required=True, help='Accessible direct HTTP(S) PDF URL')
args = parser.parse_args()
repo = pathlib.Path(__file__).resolve().parents[1]
root = pathlib.Path(tempfile.mkdtemp(prefix='tanglewood-paper-smoke-'))
sock = root / 'owner.sock'
vault = root / 'vault'
print(f'Diagnostics: {root}', flush=True)
owner = bridge = None
seq = 0


def stop():
    global owner, bridge
    for process in (bridge, owner):
        if process:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
    owner = bridge = None


def rpc(method, params):
    global seq
    seq += 1
    bridge.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': seq, 'method': method, 'params': params}) + '\n')
    bridge.stdin.flush()
    deadline = time.monotonic() + 55
    while time.monotonic() < deadline:
        if select.select([bridge.stdout], [], [], 1)[0]:
            line = bridge.stdout.readline()
            if not line:
                raise RuntimeError('bridge disconnected')
            value = json.loads(line)
            if value.get('id') == seq:
                if 'error' in value:
                    raise RuntimeError(value['error'])
                return value['result']
    raise TimeoutError(method)


def start():
    global owner, bridge
    # A previous stopped test owner's socket is ours to remove.
    sock.unlink(missing_ok=True)
    owner = subprocess.Popen([str(repo / 'target/debug/tanglewood'), '--headless', '--vault', str(vault),
                              '--mcp-socket', str(sock), '--mcp-write'], stdout=subprocess.DEVNULL)
    deadline = time.monotonic() + 10
    while not sock.exists():
        if owner.poll() is not None:
            raise RuntimeError('test owner exited')
        if time.monotonic() > deadline:
            raise TimeoutError('test owner startup')
        time.sleep(.05)
    bridge = subprocess.Popen([str(repo / 'target/debug/tanglewood-mcp'), '--socket', str(sock)],
                              stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    rpc('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {},
                       'clientInfo': {'name': 'tanglewood-paper-smoke', 'version': '1'}})
    bridge.stdin.write(json.dumps({'jsonrpc': '2.0', 'method': 'notifications/initialized'}) + '\n')
    bridge.stdin.flush()


def call(command, mutate=False):
    response = rpc('tools/call', {'name': 'execute' if mutate else 'read', 'arguments': {'command': {'Research': command}}})
    texts = [item['text'] for item in response['content'] if item.get('type') == 'text']
    value = json.loads(texts[0])
    if response.get('isError') or 'Result' not in value['outcome']:
        raise RuntimeError(value)
    return value['outcome']['Result']


def workspace(wid):
    return call({'Read': {'id': wid}})['Research']['Workspace']


def read_all(wid, sid):
    page = offset = 0
    text = bytearray()
    numbers = set()
    while True:
        result = call({'ReadSource': {'id': wid, 'source_id': sid, 'page': page, 'offset': offset, 'limit': 32768}})['Research']['Source']
        text.extend(result['text'].encode())
        numbers.add(result['number'])
        if result['next_offset'] is not None:
            offset = result['next_offset']
        elif result['next_page'] is not None:
            page, offset = result['next_page'], 0
        else:
            return bytes(text), numbers


try:
    start()
    tools = rpc('tools/list', {})
    execute = next(tool for tool in tools['tools'] if tool['name'] == 'execute')
    assert 'SavePaper' in json.dumps(execute['inputSchema'])
    call({'Create': {'title': 'Paper smoke', 'question': 'Save and read actual PDF'}}, True)
    wid = call('List')['Research']['Workspaces'][0]['id']
    w = workspace(wid)
    call({'AddImported': {'id': wid, 'expected': w['revision'], 'source': {
        'title': 'Actual PDF smoke', 'kind': 'Text', 'metadata': {'url': args.url},
        'pages': [{'number': None, 'text': 'Reference only'}]}}}, True)
    w = workspace(wid)
    sid = w['sources'][0]['id']
    call({'SavePaper': {'id': wid, 'expected': w['revision'], 'source_id': sid,
                       'input': {'Url': {'url': args.url, 'archive': True}}}}, True)
    saved = workspace(wid)
    assert len(saved['sources']) == len(saved['artifacts']) == 1
    assert saved['sources'][0]['id'] == sid
    paper = saved['sources'][0]['paper']
    assert paper['extraction_error'] is None, paper
    original = vault / saved['artifacts'][0]['managed_path']
    pdf = original.read_bytes()
    assert pdf.startswith(b'%PDF-')
    text, pages = read_all(wid, sid)
    assert len(text) > 100 and pages
    digest = hashlib.sha256(text).hexdigest()
    stop()
    start()
    assert workspace(wid) == saved
    reread, repages = read_all(wid, sid)
    assert reread == text and repages == pages and original.read_bytes() == pdf
    print(f'PASS: PDF bytes={len(pdf)}, extracted bytes={len(text)}, pages={len(pages)}, text_sha256={digest}; same source and bytes after owner restart.', flush=True)
finally:
    stop()
