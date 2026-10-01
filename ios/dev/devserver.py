#!/usr/bin/env python3
"""Servidor de desarrollo para la app de iOS.

- GET  /files/<ruta>  código web (src/, app/, popup/ desde la raíz; web/ desde ios/web)
- GET  /poll          la app espera aquí código que evaluar
- POST /result        la app devuelve el resultado
- POST /eval          la terminal manda código y espera el resultado (ver ./js)

Uso: python3 ios/dev/devserver.py  (puerto 8777)
Lanza la app con SIMCTL_CHILD_BTE_DEV=http://127.0.0.1:8777
"""
import json
import queue
import threading
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

IOS = Path(__file__).resolve().parent.parent
ROOT = IOS.parent
jobs: "queue.Queue[dict]" = queue.Queue()
results: dict = {}
cond = threading.Condition()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def send(self, code, body=b"", ctype="application/json"):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.startswith("/files/"):
            rel = self.path[len("/files/"):].split("?")[0]
            base = IOS if rel.startswith("web/") else ROOT
            file = (base / rel).resolve()
            if not str(file).startswith(str(ROOT)) or not file.is_file():
                return self.send(404, b"no")
            return self.send(200, file.read_bytes(), "text/plain; charset=utf-8")
        if self.path == "/poll":
            try:
                job = jobs.get(timeout=25)
            except queue.Empty:
                return self.send(204)
            return self.send(200, json.dumps(job).encode())
        self.send(404)

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        if self.path == "/result":
            data = json.loads(body or b"{}")
            with cond:
                results[data.get("id")] = data
                cond.notify_all()
            return self.send(200, b"{}")
        if self.path == "/eval":
            job = {"id": uuid.uuid4().hex, "code": body.decode()}
            jobs.put(job)
            with cond:
                cond.wait_for(lambda: job["id"] in results, timeout=90)
                data = results.pop(job["id"], {"error": "sin respuesta de la app (¿está abierta con BTE_DEV?)"})
            return self.send(200, json.dumps(data, ensure_ascii=False, indent=1).encode())
        self.send(404)


if __name__ == "__main__":
    print("devserver en http://127.0.0.1:8777")
    ThreadingHTTPServer(("0.0.0.0", 8777), Handler).serve_forever()
