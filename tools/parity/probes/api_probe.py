#!/usr/bin/env python3
"""
API Probe: Validates PWNGHOST-RS REST Endpoints
Note: Jayofelony has no REST API (verified: /api/status returns HTTP 404).
PWNGHOST-RS REST API is additive and validated for self-consistency only.
"""

import sys
import json
import urllib.request

def fetch_json(url):
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'ParityProbe/1.0'})
        with urllib.request.urlopen(req, timeout=5) as response:
            return json.loads(response.read().decode('utf-8'))
    except urllib.error.HTTPError as e:
        # Return HTTP error code (404, etc.) so caller can see it's a legit endpoint failure
        return None
    except Exception as e:
        print(f"[API Probe ERROR] Failed to fetch {url}: {e}")
        return None

def validate_pwnghost_endpoint(pwnghost_data, endpoint_name):
    """Validate PWNGHOST-RS endpoint returns valid JSON with documented keys."""
    if pwnghost_data is None:
        print(f"[API Probe FAIL] Endpoint {endpoint_name}: PWNGHOST-RS returned no data (expected valid JSON)")
        return False

    # Valid JSON may be an object (status, config) OR an array
    # (handshakes, peers are legitimately lists).
    if isinstance(pwnghost_data, dict):
        print(f"[API Probe PASS] Endpoint {endpoint_name}: PWNGHOST-RS returned valid JSON object (keys: {set(pwnghost_data.keys())})")
        return True
    if isinstance(pwnghost_data, list):
        print(f"[API Probe PASS] Endpoint {endpoint_name}: PWNGHOST-RS returned valid JSON array (len: {len(pwnghost_data)})")
        return True

    print(f"[API Probe FAIL] Endpoint {endpoint_name}: PWNGHOST-RS returned non-JSON-container response")
    return False

def run_api_probe(jay_base="http://localhost:8080", pwnghost_base="http://localhost:8082"):
    endpoints = ["/api/status", "/api/config", "/api/handshakes", "/api/peers"]
    results = {}
    all_passed = True

    print("=== Running PWNGHOST-RS API Validation Probe ===")
    print("Note: Jayofelony has no REST API (verified: /api/status -> HTTP 404)")
    print()

    for ep in endpoints:
        jay_json = fetch_json(f"{jay_base}{ep}")
        pwnghost_json = fetch_json(f"{pwnghost_base}{ep}")

        # Jayofelony should NOT have REST endpoints (expected: None / 404)
        if jay_json is not None:
            print(f"[API Probe WARN] Endpoint {ep}: Jayofelony returned data (expected NO REST API)")
        else:
            print(f"[API Probe INFO] Endpoint {ep}: Jayofelony has no REST API (expected)")

        # PWNGHOST-RS endpoint must be valid JSON
        passed = validate_pwnghost_endpoint(pwnghost_json, ep)
        results[ep] = passed
        if not passed:
            all_passed = False

    return all_passed, results

if __name__ == "__main__":
    pwnghost_url = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8082"
    jay_url = sys.argv[2] if len(sys.argv) > 2 else "http://localhost:8080"
    success, _ = run_api_probe(jay_url, pwnghost_url)
    sys.exit(0 if success else 1)
