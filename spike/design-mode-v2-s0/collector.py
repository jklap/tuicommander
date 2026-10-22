"""Explicit loopback-only evidence collector; no product/API integration."""
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

root = Path(__file__).resolve().parent / 'evidence'
class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        data = self.rfile.read(int(self.headers['Content-Length']))
        if self.path == '/events':
            with (root / 'events.jsonl').open('ab') as output:
                output.write(data + b'\n')
        elif self.path == '/crop':
            (root / 'crop.png').write_bytes(data)
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'ok')
HTTPServer(('127.0.0.1', 19341), Handler).serve_forever()
