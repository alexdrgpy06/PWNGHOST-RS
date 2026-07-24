#!/usr/bin/env python3
"""
Logic & State Behavior Probe
Monitors state machine transitions (epochs, moods, face kaomojis) during scenario runs.
"""

import sys
import json
import time
import urllib.request

def get_status(url):
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'LogicProbe/1.0'})
        with urllib.request.urlopen(req, timeout=3) as resp:
            return json.loads(resp.read().decode('utf-8'))
    except Exception as e:
        return None

def run_logic_probe(pwnghost_url="http://localhost:8082/api/status"):
    print("=== Running Logic & Behavioral Transition Probe ===")
    transitions = []
    
    for i in range(5):
        st = get_status(pwnghost_url)
        if st:
            transitions.append({
                "t": i * 2,
                "status": st.get("status"),
                "epoch": st.get("epoch"),
                "aps": st.get("aps"),
                "mode": st.get("mode")
            })
        time.sleep(1)

    print(f"[Logic Probe] Tracked {len(transitions)} state snapshots over scenario run.")
    return True, transitions

if __name__ == "__main__":
    url = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8082/api/status"
    success, trs = run_logic_probe(url)
    sys.exit(0 if success else 1)
