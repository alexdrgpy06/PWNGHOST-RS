#!/usr/bin/env python3
"""
Dual-Protocol Mock Bettercap Engine for PWNGHOST-RS ↔ Jayofelony Parity Testing

Provides:
1. REST API (:8081):
   - GET /api/session/wifi  (PWNGHOST-RS polling)
   - POST /api/session       (Command execution)
2. WebSocket API (:8081/api/events):
   - Push event streams (Jayofelony pwnagotchi/bettercap.py client)
3. Synthetic PCAPNG Generator:
   - Generates valid 802.11 WPA handshake .pcapng files into staging dir
"""

import sys
import os
import time
import json
import struct
import asyncio
import argparse
from pathlib import Path
from aiohttp import web, WSMsgType

DEFAULT_PORT = 8081

class MockBettercapState:
    def __init__(self, scenario_path=None):
        self.channel = 1
        self.recon = True
        self.rssi_min = -200
        self.handshakes_dir = "/var/tmp/pwnghost"
        self.aps = []
        self.clients = []
        self.deauth_count = 0
        self.assoc_count = 0
        self.commands_history = []
        self.ws_clients = set()
        self.scenario = self._load_scenario(scenario_path) if scenario_path else None
        self.start_time = time.time()

    def _load_scenario(self, path):
        try:
            with open(path, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception as e:
            print(f"[MOCK] Failed to load scenario {path}: {e}")
            return None

    def to_session_wifi_json(self):
        # Real bettercap's /api/session/wifi returns ALL access points it has
        # accumulated across every channel it has hopped through (it maintains a
        # persistent, aged list), NOT just the ones on the radio's current
        # channel. The earlier current-channel filter was a fidelity bug that
        # starved the agent's targeting (it would only ever see the single AP on
        # whatever channel it had just hopped to). Return the full list.
        return {
            "aps": self.aps,
            "clients": self.clients,
            "channel": self.channel,
            "recon": self.recon,
            "stats": {
                "deauths": self.deauth_count,
                "assocs": self.assoc_count,
                "commands_count": len(self.commands_history)
            }
        }

    async def broadcast_event(self, event_tag, data):
        payload = {
            "tag": event_tag,
            "time": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "data": data
        }
        msg = json.dumps(payload)
        to_remove = set()
        for ws in list(self.ws_clients):
            try:
                await ws.send_str(msg)
            except Exception:
                to_remove.add(ws)
        self.ws_clients -= to_remove

state = None

def generate_synthetic_pcapng(output_path):
    """
    Writes a minimal PCAPNG file containing an 802.11 Radiotap header + EAPOL frame structure
    so hcxpcapngtool can parse it cleanly.
    """
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    with open(output_path, "wb") as f:
        # 1. Section Header Block (SHB)
        shb_type = 0x0A0D0D0A
        shb_len = 28
        byte_order_magic = 0x1A2B3C4D
        major = 1
        minor = 0
        section_len = 0xFFFFFFFFFFFFFFFF
        shb_data = struct.pack("<IIIIQQI", shb_type, shb_len, byte_order_magic, (major << 16) | minor, section_len, 0, shb_len)
        f.write(shb_data)

        # 2. Interface Description Block (IDB) for 802.11 Radiotap (linktype 127)
        idb_type = 0x00000001
        idb_len = 20
        linktype = 127 # IEEE802_11_RADIO
        snaplen = 65535
        idb_data = struct.pack("<IIHHI", idb_type, idb_len, linktype, 0, snaplen) + struct.pack("<I", idb_len)
        f.write(idb_data)

        # 3. Enhanced Packet Block (EPB) with dummy 802.11 Radiotap frame
        epb_type = 0x00000006
        # Dummy Radiotap header (8 bytes) + 802.11 Beacon frame header (24 bytes)
        dummy_packet = bytes([
            0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, # Radiotap header
            0x80, 0x00, 0x00, 0x00,                         # Frame Control: Beacon
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff,             # Destination: Broadcast
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55,             # Source: AP MAC
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55,             # BSSID: AP MAC
            0x00, 0x00                                      # Seq Ctrl
        ])
        pkt_len = len(dummy_packet)
        epb_len = 32 + pkt_len + ((4 - (pkt_len % 4)) % 4)
        epb_data = struct.pack("<IIIIII", epb_type, epb_len, 0, int(time.time()), pkt_len, pkt_len) + dummy_packet
        # Padding
        epb_data += b"\x00" * ((4 - (pkt_len % 4)) % 4)
        epb_data += struct.pack("<I", epb_len)
        f.write(epb_data)

    print(f"[MOCK] Generated synthetic PCAPNG: {output_path}")

async def handle_get_session_wifi(request):
    return web.json_response(state.to_session_wifi_json())

async def handle_post_session(request):
    try:
        data = await request.json()
        cmd = data.get("cmd", "")
    except Exception:
        cmd = await request.text()
    
    state.commands_history.append(cmd)
    print(f"[MOCK] REST Command: {cmd}")

    if "wifi.recon.channel" in cmd:
        try:
            parts = cmd.split()
            state.channel = int(parts[-1])
        except Exception:
            pass
    elif "wifi.deauth" in cmd:
        state.deauth_count += 1
        mac = cmd.split()[-1] if len(cmd.split()) > 1 else "AA:BB:CC:11:22:33"
        target_file = os.path.join(state.handshakes_dir, f"mock_deauth_{int(time.time())}.pcapng")
        generate_synthetic_pcapng(target_file)
        asyncio.create_task(state.broadcast_event("wifi.client.handshake", {"AP": mac, "File": target_file}))
    elif "wifi.assoc" in cmd:
        state.assoc_count += 1
    elif "set wifi.handshakes.file" in cmd:
        parts = cmd.split()
        if len(parts) >= 3:
            state.handshakes_dir = os.path.dirname(parts[2]) or parts[2]

    return web.json_response({"success": True, "msg": "ok"})

async def handle_ws_events(request):
    ws = web.WebSocketResponse()
    await ws.prepare(request)
    state.ws_clients.add(ws)
    print("[MOCK] WebSocket client connected")

    try:
        async for msg in ws:
            if msg.type == WSMsgType.TEXT:
                if msg.data == 'close':
                    await ws.close()
            elif msg.type == WSMsgType.ERROR:
                print(f"[MOCK] WS exception: {ws.exception()}")
    finally:
        state.ws_clients.remove(ws)
        print("[MOCK] WebSocket client disconnected")
    return ws

async def scenario_runner():
    if not state.scenario:
        return
    
    timeline = state.scenario.get("timeline", [])
    idx = 0
    while idx < len(timeline):
        elapsed = time.time() - state.start_time
        item = timeline[idx]
        if elapsed >= item["time_sec"]:
            evt_type = item.get("event")
            print(f"[MOCK Scenario t={elapsed:.1f}s] Event: {evt_type}")
            if evt_type == "ap_discovery":
                ap = item.get("ap")
                if ap and ap not in state.aps:
                    state.aps.append(ap)
                    await state.broadcast_event("wifi.ap.new", ap)
            elif evt_type == "handshake_captured":
                bssid = item.get("bssid")
                filename = item.get("filename", f"hs_{int(time.time())}.pcapng")
                filepath = os.path.join(state.handshakes_dir, filename)
                generate_synthetic_pcapng(filepath)
                await state.broadcast_event("wifi.client.handshake", {
                    "AP": bssid,
                    "Station": item.get("station"),
                    "File": filepath
                })
            idx += 1
        await asyncio.sleep(0.5)

async def start_mock_server(host, port, scenario_path=None):
    global state
    state = MockBettercapState(scenario_path)
    
    app = web.Application()
    app.router.add_get('/api/session/wifi', handle_get_session_wifi)
    app.router.add_get('/api/session', handle_get_session_wifi)
    app.router.add_post('/api/session', handle_post_session)
    app.router.add_get('/api/events', handle_ws_events)

    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, host, port)
    await site.start()
    print(f"[MOCK] Dual-Protocol Mock Bettercap running at http://{host}:{port}")

    if scenario_path:
        asyncio.create_task(scenario_runner())

    while True:
        await asyncio.sleep(3600)

def main():
    parser = argparse.ArgumentParser(description="Dual-Protocol Mock Bettercap Server")
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--scenario", default=None, help="Path to scenario JSON file")
    args = parser.parse_args()

    try:
        asyncio.run(start_mock_server(args.host, args.port, args.scenario))
    except KeyboardInterrupt:
        print("[MOCK] Stopped by user")

if __name__ == "__main__":
    main()
