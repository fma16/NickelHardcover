#!/usr/bin/env python3
"""Direct Readwise calls for verification. Token from the environment only.
Usage: rw.py METHOD PATH [json-body]   (PATH relative to https://readwise.io)"""
import json, os, sys, urllib.request, urllib.error
method, path = sys.argv[1], sys.argv[2]
body = sys.argv[3].encode() if len(sys.argv) > 3 else None
req = urllib.request.Request("https://readwise.io" + path, data=body, method=method, headers={
    "Authorization": "Token " + os.environ["Readwise_test_account"],
    "Content-Type": "application/json", "User-Agent": "Bookwise-Nickel-review/0.1"})
try:
    with urllib.request.urlopen(req, timeout=30) as r:
        print("HTTP", r.status); text = r.read().decode()
except urllib.error.HTTPError as e:
    print("HTTP", e.code, {k: v for k, v in e.headers.items() if k.lower() == "retry-after"}); text = e.read().decode()
try:
    print(json.dumps(json.loads(text), indent=1, ensure_ascii=False))
except ValueError:
    print(text[:2000])
