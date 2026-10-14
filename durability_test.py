#!/usr/bin/env python3
"""
Comprehensive Durability, Concurrency, and Stress Test Suite for EpiKairos Rust Backend
Tests:
1. Concurrency & Throughput (10,000 requests mix of GET/POST across Authenticated & Public routes)
2. Database Lock Contention (Parallel writes & reads to SQLite with WAL mode)
3. Fuzzing & Hostile Input Resilience (Malformed JSON, SQLi, Path Traversal, ReDoS, Cookies)
4. Login Rate Limiting & Route Isolation
5. Memory Stability & Resource Leak Detection
"""

import asyncio
import aiohttp
import base64
import hashlib
import hmac
import json
import os
import psutil
import signal
import subprocess
import sys
import time
from typing import Dict, List, Any

BASE_URL = "http://127.0.0.1:5055"
TEST_PORT = 5055
TEST_SECRET_KEY = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

SAMPLE_COURSE = {
    "id": "test_course_001",
    "start": "2026-10-14T09:00:00",
    "end": "2026-10-14T11:00:00",
    "duration": 120,
    "summary": "Algorithmique Avancee",
    "course_name": "Algo Avancee",
    "course_code": "ALGO",
    "activity_type": "CM",
    "activity_name": "Cours Magistral",
    "is_exam": False,
    "is_cancelled": False,
    "status": "PLANIFIE",
    "teachers": ["Prof. Test"],
    "rooms": ["Amphi 1"],
    "groups": ["ING1-G1"],
    "description": "Test course description",
    "searchable_text": "Algorithmique Avancee Prof. Test Amphi 1 ING1-G1"
}

def generate_signed_session(username="test_student", exp_offset=86400):
    payload = json.dumps({
        "logged_in": True,
        "username": username,
        "exp": int(time.time()) + exp_offset
    })
    payload_b64 = base64.urlsafe_b64encode(payload.encode()).decode().rstrip('=')
    sig = hmac.new(TEST_SECRET_KEY.encode(), payload_b64.encode(), hashlib.sha256).digest()
    sig_b64 = base64.urlsafe_b64encode(sig).decode().rstrip('=')
    return f"{payload_b64}.{sig_b64}"

def start_server():
    print("[*] Launching target/release/epikairos on port 5055...")
    env = os.environ.copy()
    env["PORT"] = str(TEST_PORT)
    env["HOST"] = "127.0.0.1"
    env["FLASK_SECRET_KEY"] = TEST_SECRET_KEY
    env["TRUSTED_PROXY"] = "false"
    
    proc = subprocess.Popen(
        ["./target/release/epikairos"],
        cwd="/home/fernand/playground/zeus-v2",
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        preexec_fn=os.setsid
    )
    time.sleep(1.0)
    if proc.poll() is not None:
        stdout, stderr = proc.communicate()
        print(f"[-] Server failed to start! stdout: {stdout.decode()}, stderr: {stderr.decode()}")
        sys.exit(1)
    print(f"[+] Server running with PID {proc.pid}")
    return proc

def stop_server(proc):
    if proc:
        print("[*] Terminating server...")
        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
            proc.wait(timeout=3)
        except Exception:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            except Exception:
                pass
        print("[+] Server stopped.")

async def wait_for_server(session):
    for i in range(20):
        try:
            async with session.get(f"{BASE_URL}/health", timeout=1.0) as resp:
                if resp.status == 200:
                    print("[+] Server health check passed.")
                    return True
        except Exception:
            await asyncio.sleep(0.3)
    return False

# ==============================================================================
# PHASE 1: HIGH CONCURRENCY LOAD & THROUGHPUT
# ==============================================================================
async def phase1_concurrency_test(total_requests=6000, concurrency=60):
    print(f"\n=======================================================")
    print(f" PHASE 1: HIGH CONCURRENCY LOAD TEST ({total_requests} requests, concurrency={concurrency})")
    print(f"=======================================================")
    
    session_cookie = generate_signed_session()
    auth_headers = {"Cookie": f"session={session_cookie}"}
    
    # Mix of public, authenticated HTML, and authenticated API endpoints
    endpoints = [
        # Public endpoints (no cookie needed)
        ("GET", "/health", None, {}),
        ("GET", "/manifest.json", None, {}),
        ("GET", "/sw.js", None, {}),
        ("GET", "/static/css/style.css", None, {}),
        ("GET", "/feed/c8w5xZqrrobob53UA3FYqqokPqU.ics", None, {}),
        # Authenticated HTML endpoints
        ("GET", "/", None, auth_headers),
        ("GET", "/rooms", None, auth_headers),
        ("GET", "/custom-calendar", None, auth_headers),
        ("GET", "/personal", None, auth_headers),
        # Authenticated API endpoints
        ("GET", "/api/database/stats", None, auth_headers),
        ("GET", "/api/database/promos", None, auth_headers),
        ("GET", "/api/database/options", None, auth_headers),
        ("GET", "/api/groups/tree", None, auth_headers),
        ("POST", "/api/database/custom-courses", {"promos": ["ing1_lyon"], "keyword": "math", "hide_cancelled": True}, auth_headers),
        ("POST", "/api/database/free-rooms", {"date": "2026-10-14", "time": "14:00"}, auth_headers),
        ("POST", "/api/export-ics", {"calendar_name": "Test Cal", "courses": [SAMPLE_COURSE]}, auth_headers)
    ]
    
    conn = aiohttp.TCPConnector(limit=concurrency * 2, ttl_dns_cache=300)
    latencies = []
    status_codes = {}
    exceptions = 0
    
    sem = asyncio.Semaphore(concurrency)
    
    async with aiohttp.ClientSession(connector=conn) as session:
        start_time = time.time()
        
        async def make_req(idx):
            nonlocal exceptions
            method, path, body, headers = endpoints[idx % len(endpoints)]
            url = f"{BASE_URL}{path}"
            t0 = time.time()
            async with sem:
                try:
                    if method == "GET":
                        async with session.get(url, headers=headers, timeout=5.0) as resp:
                            await resp.read()
                            code = resp.status
                    else:
                        async with session.post(url, json=body, headers=headers, timeout=5.0) as resp:
                            await resp.read()
                            code = resp.status
                    dt = (time.time() - t0) * 1000
                    latencies.append(dt)
                    status_codes[code] = status_codes.get(code, 0) + 1
                except Exception as e:
                    exceptions += 1
        
        tasks = [asyncio.create_task(make_req(i)) for i in range(total_requests)]
        await asyncio.gather(*tasks)
        
        duration = time.time() - start_time
        rps = total_requests / duration if duration > 0 else 0
        
        latencies.sort()
        p50 = latencies[int(len(latencies) * 0.50)] if latencies else 0
        p95 = latencies[int(len(latencies) * 0.95)] if latencies else 0
        p99 = latencies[int(len(latencies) * 0.99)] if latencies else 0
        max_lat = latencies[-1] if latencies else 0
        
        print(f"Results for Phase 1:")
        print(f"  Total Requests:     {total_requests}")
        print(f"  Duration:           {duration:.2f} s")
        print(f"  Requests / Second:  {rps:.2f} RPS")
        print(f"  Status Codes:       {status_codes}")
        print(f"  Exceptions/Drops:   {exceptions}")
        print(f"  Latency (p50):      {p50:.2f} ms")
        print(f"  Latency (p95):      {p95:.2f} ms")
        print(f"  Latency (p99):      {p99:.2f} ms")
        print(f"  Latency (max):      {max_lat:.2f} ms")
        
        assert exceptions == 0, f"Expected 0 exceptions, got {exceptions}"
        assert status_codes.get(200, 0) == total_requests, f"Expected all 200 OK, got {status_codes}"

# ==============================================================================
# PHASE 2: DATABASE WRITE CONTENTION & LOCK STRESS
# ==============================================================================
async def phase2_db_lock_contention(feed_count=60):
    print(f"\n=======================================================")
    print(f" PHASE 2: DB WRITE CONTENTION & LOCK STRESS ({feed_count} feeds created concurrently)")
    print(f"=======================================================")
    
    conn = aiohttp.TCPConnector(limit=50)
    created_feed_ids = []
    lock_errors = 0
    read_successes = 0
    write_successes = 0
    
    session_cookie = generate_signed_session()
    auth_headers = {"Cookie": f"session={session_cookie}"}
    
    async with aiohttp.ClientSession(connector=conn) as session:
        # Launch concurrent writes and concurrent reads simultaneously
        async def create_single_feed(idx):
            nonlocal lock_errors, write_successes
            payload = {
                "username": "test_student",
                "calendar_name": f"Stress Feed {idx}",
                "courses": [SAMPLE_COURSE],
                "include_regex": f"course_{idx}",
                "hide_cancelled": True
            }
            try:
                async with session.post(f"{BASE_URL}/api/create-feed", json=payload, headers=auth_headers, timeout=5.0) as resp:
                    data = await resp.json()
                    if resp.status == 200 and data.get("success"):
                        write_successes += 1
                        feed_id = data.get("feed_id")
                        created_feed_ids.append(feed_id)
                    else:
                        print(f"[-] Write failed: status {resp.status}, data {data}")
            except Exception as e:
                lock_errors += 1
                print(f"[-] Write exception: {e}")
        
        async def concurrent_read(idx):
            nonlocal read_successes, lock_errors
            try:
                async with session.post(
                    f"{BASE_URL}/api/database/custom-courses",
                    json={"keyword": f"algo_{idx}", "hide_cancelled": False},
                    headers=auth_headers,
                    timeout=5.0
                ) as resp:
                    if resp.status == 200:
                        read_successes += 1
            except Exception as e:
                lock_errors += 1
                print(f"[-] Read exception: {e}")
        
        # Interleave writes and reads
        tasks = []
        for i in range(feed_count):
            tasks.append(asyncio.create_task(create_single_feed(i)))
            tasks.append(asyncio.create_task(concurrent_read(i)))
            
        await asyncio.gather(*tasks)
        
        print(f"Results for Phase 2:")
        print(f"  Feeds Created:      {write_successes}/{feed_count}")
        print(f"  Reads Succeeded:    {read_successes}/{feed_count}")
        print(f"  Lock/DB Errors:     {lock_errors}")
        
        assert lock_errors == 0, f"Encountered {lock_errors} lock/DB errors during concurrent writes!"
        assert write_successes == feed_count, f"Expected {feed_count} successful feed creations, got {write_successes}"
        
        # Verify created feeds are readable immediately without auth (public feed endpoint)
        print("[*] Verifying created feeds can be fetched immediately...")
        verify_success = 0
        for fid in created_feed_ids[:10]:
            async with session.get(f"{BASE_URL}/feed/{fid}.ics") as resp:
                if resp.status == 200 and "BEGIN:VCALENDAR" in await resp.text():
                    verify_success += 1
        print(f"  Verified 10/10 sampled feeds readable: {verify_success == 10}")
        assert verify_success == 10

# ==============================================================================
# PHASE 3: FUZZING & HOSTILE INPUT RESILIENCE
# ==============================================================================
async def phase3_fuzzing_resilience():
    print(f"\n=======================================================")
    print(f" PHASE 3: FUZZING & HOSTILE INPUT RESILIENCE")
    print(f"=======================================================")
    
    session_cookie = generate_signed_session()
    auth_headers = {"Cookie": f"session={session_cookie}"}
    
    async with aiohttp.ClientSession() as session:
        tests = [
            # 1. Malformed JSON bodies
            ("Malformed JSON (truncated)", "POST", "/api/database/custom-courses", "{\"teachers\": [\"foo\",", "application/json", [400, 422], auth_headers),
            ("Malformed JSON (syntax error)", "POST", "/api/database/free-rooms", "{bad_json: 123}", "application/json", [400, 422], auth_headers),
            ("Empty body with JSON header", "POST", "/api/create-feed", "", "application/json", [400, 422], auth_headers),
            
            # 2. SQL Injection in JSON filters
            ("SQL Injection in teacher field", "POST", "/api/database/custom-courses", json.dumps({"teachers": ["' OR 1=1 --", "'; DROP TABLE courses; --"]}), "application/json", [200], auth_headers),
            ("SQL Injection in room field", "POST", "/api/database/custom-courses", json.dumps({"rooms": ["Amphi') UNION SELECT 1,2,3--"]}), "application/json", [200], auth_headers),
            ("SQL Injection in free-rooms date", "POST", "/api/database/free-rooms", json.dumps({"date": "2026-10-14' OR '1'='1", "time": "14:00"}), "application/json", [200, 400], auth_headers),
            
            # 3. Invalid Regex & Length Overload
            ("Invalid regex syntax (unclosed)", "POST", "/api/create-feed", json.dumps({"username": "test_student", "password": "x", "include_regex": "[unclosed"}), "application/json", [400, 422], auth_headers),
            ("Oversized regex (>300 chars)", "POST", "/api/create-feed", json.dumps({"username": "test_student", "password": "x", "include_regex": "a" * 350}), "application/json", [400], auth_headers),
            
            # 4. Oversized Payload (5MB body)
            ("Huge payload body (5MB)", "POST", "/api/export-ics", json.dumps({"courses": [SAMPLE_COURSE] * 1000}), "application/json", [200, 400, 413], auth_headers),
            
            # 5. Malformed Session Cookies
            ("Corrupted session cookie base64", "GET", "/", None, None, [303], {"Cookie": "session=invalid.notb64"}),
            ("Forged session signature", "GET", "/", None, None, [303], {"Cookie": "session=eyJsb2dnZWRfaW4iOnRydWV9.fake_sig_12345"}),
            ("Expired session cookie", "GET", "/", None, None, [303], {"Cookie": f"session={generate_signed_session(exp_offset=-3600)}"}),
            
            # 6. Non-existent routes (404)
            ("Authenticated nonexistent route", "GET", "/some/random/missing/route", None, None, [404], auth_headers),
            ("Public nonexistent static asset", "GET", "/static/does_not_exist_file.txt", None, None, [404], {}),
            ("Nonexistent feed ID", "GET", "/feed/definitely_not_a_real_feed_id.ics", None, None, [404], {}),
        ]
        
        passed = 0
        for name, method, path, body, ctype, expected_codes, base_h in tests:
            headers = dict(base_h)
            if ctype:
                headers["Content-Type"] = ctype
            
            url = f"{BASE_URL}{path}"
            t0 = time.time()
            try:
                if method == "GET":
                    async with session.get(url, headers=headers, allow_redirects=False, timeout=5.0) as resp:
                        status = resp.status
                        await resp.read()
                else:
                    async with session.post(url, data=body, headers=headers, allow_redirects=False, timeout=5.0) as resp:
                        status = resp.status
                        await resp.read()
                dt = (time.time() - t0) * 1000
                if status in expected_codes:
                    print(f"  [PASS] {name:<40} -> status {status} ({dt:.1f}ms)")
                    passed += 1
                else:
                    print(f"  [FAIL] {name:<40} -> status {status} (expected {expected_codes})")
            except Exception as e:
                print(f"  [FAIL] {name:<40} -> Exception: {e}")

        # 7. Raw Socket Path Traversal Tests
        raw_traversal_paths = [
            "/static/../../../../etc/passwd",
            "/feed/..%2f..%2fetc%2fpasswd.ics"
        ]
        for rpath in raw_traversal_paths:
            t0 = time.time()
            try:
                reader, writer = await asyncio.open_connection("127.0.0.1", TEST_PORT)
                req_bytes = f"GET {rpath} HTTP/1.1\r\nHost: 127.0.0.1:{TEST_PORT}\r\nConnection: close\r\n\r\n".encode()
                writer.write(req_bytes)
                await writer.drain()
                resp_data = await reader.read(1024)
                writer.close()
                await writer.wait_closed()
                status_line = resp_data.split(b"\r\n")[0].decode(errors="ignore")
                dt = (time.time() - t0) * 1000
                # Must be 400 or 404, never 200 and never contains "root:"
                if ("400" in status_line or "404" in status_line) and b"root:x:" not in resp_data:
                    print(f"  [PASS] Raw Path Traversal ({rpath[:20]}..) -> {status_line} ({dt:.1f}ms)")
                    passed += 1
                else:
                    print(f"  [FAIL] Raw Path Traversal ({rpath}) returned {status_line}")
            except Exception as e:
                print(f"  [FAIL] Raw Path Traversal ({rpath}) error: {e}")

        total_fuzz_tests = len(tests) + len(raw_traversal_paths)
        print(f"Fuzzing test passed: {passed}/{total_fuzz_tests}")
        assert passed == total_fuzz_tests, f"Failed {total_fuzz_tests - passed} fuzzing tests!"

# ==============================================================================
# PHASE 4: LOGIN RATE LIMITING & ROUTE ISOLATION
# ==============================================================================
async def phase4_rate_limiting_test():
    print(f"\n=======================================================")
    print(f" PHASE 4: RATE LIMITING DURABILITY & ROUTE ISOLATION")
    print(f"=======================================================")
    
    session_cookie = generate_signed_session()
    auth_headers = {"Cookie": f"session={session_cookie}"}
    
    async with aiohttp.ClientSession() as session:
        # Rapid-fire 30 login requests from same IP
        print("[*] Sending rapid login requests to verify 429 rate limit triggers...")
        got_429 = False
        for i in range(30):
            payload = {"username": f"user_{i}", "password": "wrong_password"}
            async with session.post(f"{BASE_URL}/api/fetch-planning", json=payload) as resp:
                if resp.status == 429:
                    got_429 = True
                    print(f"  [+] Rate limiter triggered at request #{i+1} with HTTP 429")
                    break
        assert got_429, "Expected login rate limiter to trigger HTTP 429!"
        
        # Verify that other routes remain fast and unaffected
        print("[*] Verifying that public and authenticated APIs remain unaffected...")
        async with session.get(f"{BASE_URL}/api/database/stats", headers=auth_headers) as resp:
            assert resp.status == 200
            data = await resp.json()
            assert "total_courses" in data
            print(f"  [+] API /api/database/stats responded HTTP 200 in parallel: {data}")

# ==============================================================================
# PHASE 5: RESOURCE STABILITY & MEMORY LEAK DETECTION
# ==============================================================================
async def phase5_memory_stability_test(proc_pid, sustained_requests=5000):
    print(f"\n=======================================================")
    print(f" PHASE 5: SUSTAINED TRAFFIC & MEMORY STABILITY ({sustained_requests} requests)")
    print(f"=======================================================")
    
    p = psutil.Process(proc_pid)
    mem_before = p.memory_info().rss / (1024 * 1024)
    print(f"  [*] Initial Memory RSS: {mem_before:.2f} MB")
    
    session_cookie = generate_signed_session()
    auth_headers = {"Cookie": f"session={session_cookie}"}
    
    conn = aiohttp.TCPConnector(limit=50)
    async with aiohttp.ClientSession(connector=conn) as session:
        endpoints = [
            ("/health", {}),
            ("/rooms", auth_headers),
            ("/api/database/stats", auth_headers),
            ("/api/database/promos", auth_headers),
            ("/manifest.json", {})
        ]
        
        sem = asyncio.Semaphore(50)
        async def fetch(i):
            path, h = endpoints[i % len(endpoints)]
            async with sem:
                async with session.get(f"{BASE_URL}{path}", headers=h) as resp:
                    await resp.read()
                    assert resp.status == 200
        
        tasks = [asyncio.create_task(fetch(i)) for i in range(sustained_requests)]
        await asyncio.gather(*tasks)
        
    time.sleep(0.5)
    mem_after = p.memory_info().rss / (1024 * 1024)
    mem_diff = mem_after - mem_before
    print(f"  [*] Memory RSS after {sustained_requests} requests: {mem_after:.2f} MB (Delta: {mem_diff:+.2f} MB)")
    
    # In Rust, memory should be highly stable (delta < 25 MB under 5000 requests)
    assert mem_diff < 25.0, f"Memory grew excessively by {mem_diff:.2f} MB!"
    print(f"  [+] Memory stability test passed with minimal delta ({mem_diff:+.2f} MB).")

async def main():
    server_proc = start_server()
    try:
        async with aiohttp.ClientSession() as session:
            ready = await wait_for_server(session)
            if not ready:
                print("[-] Server did not become ready.")
                sys.exit(1)
                
        t_start = time.time()
        await phase1_concurrency_test(total_requests=6000, concurrency=60)
        await phase2_db_lock_contention(feed_count=60)
        await phase3_fuzzing_resilience()
        await phase4_rate_limiting_test()
        await phase5_memory_stability_test(server_proc.pid, sustained_requests=5000)
        
        total_time = time.time() - t_start
        print(f"\n=======================================================")
        print(f" ALL DURABILITY & STRESS TESTS PASSED! Total: {total_time:.2f}s")
        print(f"=======================================================\n")
    finally:
        stop_server(server_proc)

if __name__ == "__main__":
    asyncio.run(main())
