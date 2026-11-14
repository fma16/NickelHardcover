#!/usr/bin/env python3
"""Direct Hardcover GraphQL calls for verification. Token from the environment only.
Usage: hc.py 'query { ... }' [json-variables]"""
import json, os, sys, urllib.request, urllib.error
payload = {"query": sys.argv[1]}
if len(sys.argv) > 2:
    payload["variables"] = json.loads(sys.argv[2])
req = urllib.request.Request("https://api.hardcover.app/v1/graphql", data=json.dumps(payload).encode(), method="POST", headers={
    "authorization": "Bearer " + os.environ["Hardcover_test_account"],
    "content-type": "application/json", "user-agent": "Bookwise-Nickel-review/0.1"})
try:
    with urllib.request.urlopen(req, timeout=30) as r:
        print("HTTP", r.status); text = r.read().decode()
except urllib.error.HTTPError as e:
    print("HTTP", e.code); text = e.read().decode()
try:
    print(json.dumps(json.loads(text), indent=1, ensure_ascii=False))
except ValueError:
    print(text[:2000])
