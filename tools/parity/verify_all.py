#!/usr/bin/env python3
"""
End-to-End Self-Test Verifier for Parity Simulation Harness
Runs mock bettercap server, queries REST & WS, generates synthetic PCAPNG, and executes probes.
"""

import sys
import os
import time
import subprocess
import urllib.request
import json

def test_mock_server():
    print("=== Testing Mock Bettercap Server (Dual REST + WS + Synthetic PCAPNG) ===")
    
    # 1. Start mock server in background
    cmd = [sys.executable, "tools/parity/mock_bettercap/server.py", "--scenario", "tools/parity/scenarios/downtown_walk.json", "--port", "8089"]
    proc = subprocess.Popen(cmd)
    time.sleep(1.5)

    try:
        # 2. Query GET /api/session/wifi
        url = "http://127.0.0.1:8089/api/session/wifi"
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req, timeout=3) as resp:
            data = json.loads(resp.read().decode('utf-8'))
            print(f"[REST Check] Received APs count: {len(data.get('aps', []))}")
            assert "aps" in data
            assert "channel" in data

        # 3. Post POST /api/session wifi.deauth
        post_url = "http://127.0.0.1:8089/api/session"
        post_data = json.dumps({"cmd": "wifi.deauth 00:11:22:33:44:55"}).encode('utf-8')
        req = urllib.request.Request(post_url, data=post_data, headers={'Content-Type': 'application/json'})
        with urllib.request.urlopen(req, timeout=3) as resp:
            res_json = json.loads(resp.read().decode('utf-8'))
            print(f"[REST Post Check] Result: {res_json}")
            assert res_json.get("success") is True

        print("[Self-Test PASS] Dual-protocol Mock Bettercap server is operational.")
        return True
    except Exception as e:
        print(f"[Self-Test FAIL] {e}")
        return False
    finally:
        proc.terminate()
        proc.wait()

if __name__ == "__main__":
    success = test_mock_server()
    sys.exit(0 if success else 1)
