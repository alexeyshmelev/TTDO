#!/usr/bin/env python3

import json
import os
import subprocess
import sys
import tempfile
import threading
from argparse import ArgumentParser
from datetime import datetime
from pathlib import Path


def parse_socks_ports_range(text):
    if len(text) <= 3 or text[0] != '(' or text[-1] != ')' or ',' not in text:
        raise ValueError('invalid syntax')

    (first, last) = text[1:-1].split(',')
    return int(first), int(last)


PARSER = ArgumentParser()
PARSER.add_argument("-j", "--jobs", default=1, help="Parallel jobs number. Default is 1.", type=int)
PARSER.add_argument("-x", "--proxy",
                    help="""
                    URL of the endpoint as a proxy for the HTTP transfer tests.
                    The syntax is the same as the corresponding curl option.
                    In case it's not specified, it's assumed that network traffic is tunneled
                    through an endpoint (for example, via a VPN client).
                    If proxy authorization is required, the URL should contain the credentials
                    (e.g., https://login:password@myproxy.org).
                    """)
PARSER.add_argument("-d", "--download", help="URL for the file download test.", type=str)
PARSER.add_argument("-u", "--upload",
                    help="""
                    URL for the file upload test. Shouldn't contain a file name, it is generated automatically.
                    """,
                    type=str)
PARSER.add_argument("-o", "--output", help="Output file. Filled with JSON encoded results.", type=str)
PARSER.add_argument("--proto", help="HTTP version to use", type=str, choices=["http2", "http3"])
PARSER.add_argument("--socks-ports-range",
                    help="""
                    Syntax: (int,int) (e.g., `--socks-ports-range (1,42)`)
                    Has sense in case a SOCKS5 proxy is specified (i.e., `--proxy socks5[h]://...`).
                    The proxy URL MUST NOT contain a port number. Instead, the script generates
                    parallel requests to destination through the SOCKS5 proxies running on the same
                    host.
                    For example, `--download http://192.0.2.10:8080/file.txt --proxy socks5://127.0.0.1 --socks-ports-range (1080,1081)`
                    runs one request for `http://192.0.2.10:8080/file.txt` through each of 2 SOCKS5
                    proxies (`127.0.0.1:1080` and `127.0.0.1:1081`) in parallel.
                    Adding `--jobs N` argument causes repeating the request N times through each proxy (i.e.,
                    `--jobs 2 --download http://192.0.2.10:8080/file.txt --proxy socks5://127.0.0.1 --socks-ports-range (1080,1081)`
                    runs the request 2 times through each proxy).
                    """,
                    type=parse_socks_ports_range)

ARGS = PARSER.parse_args()


class HttpMeasurements:
    jobs_num: int = 0
    failed_num: int = 0
    avg_speed_MBps: float = 0.0
    errors: [str] = []

    def __init__(self, jobs_num, speed_samples, errors):
        assert jobs_num == len(speed_samples) + len(errors)

        self.jobs_num = jobs_num
        self.failed_num = len(errors)
        if len(speed_samples) > 0:
            self.avg_speed_MBps = sum(speed_samples) / len(speed_samples)
        self.errors = errors

    def __str__(self):
        return f"""
        Jobs number:       {self.jobs_num}
        Failed jobs:       {self.failed_num}
        Average speed:     {self.avg_speed_MBps:.2f}MB/s
        """

    def __repr__(self):
        return str(self)


def run_file_download_test(url, parallel_files, proto, proxy_url):
    print(f"{datetime.now().time()} | Running file download test...")

    args = ["curl", "--output", "/dev/null", "--fail", "--insecure", url]
    if proxy_url is not None:
        args.extend(["--proxy-insecure", "--proxy", proxy_url])
    if proto == "http2":
        args.append("--http2-prior-knowledge")
    elif proto == "http3":
        args.append("--http3-only")
    processes = []
    output_files = []
    print(f"{datetime.now().time()} | Running command '{args}' in parallel {parallel_files}...")
    for i in range(0, parallel_files):
        f = tempfile.NamedTemporaryFile(
            mode='w+', prefix=f'curl-dl-{i}-', suffix='.log', delete=False)
        proc = subprocess.Popen(args, stdout=f, stderr=subprocess.STDOUT)
        processes.append(proc)
        output_files.append(f)

    speeds_MBps = []
    errors = []
    for i, p in enumerate(processes):
        p.wait()

        output_files[i].close()
        with open(output_files[i].name, 'r') as log:
            output = log.read()

        if p.returncode != 0:
            print(f"{datetime.now().time()} | Transfer failed:\n{output}")
            errors.append(output)
            continue

        speed_str = output.splitlines()[-1].split()[6]
        if 'k' in speed_str:
            speeds_MBps.append(float(speed_str.replace('k', '')) / 1024)
        elif 'M' in speed_str:
            speeds_MBps.append(float(speed_str.replace('M', '')))
        elif 'G' in speed_str:
            speeds_MBps.append(1024 * float(speed_str.replace('G', '')))
        else:
            speeds_MBps.append(float(speed_str) / (1024 * 1024))

    for f in output_files:
        try:
            os.unlink(f.name)
        except OSError:
            pass

    print(f"{datetime.now().time()} | File download test is done")
    return HttpMeasurements(
        parallel_files,
        speeds_MBps,
        errors,
    )


def run_file_upload_test(url, parallel_files, proto, proxy_url):
    print(f"{datetime.now().time()} | Running file upload test...")

    args = ["curl", "-X", "PUT", "-T", os.environ['UPLOAD_FILENAME'], "--fail", "--insecure"]
    if proto == "http2":
        args.append("--http2-prior-knowledge")
    elif proto == "http3":
        args.append("--http3-only")
    if proxy_url is not None:
        args.extend(["--proxy-insecure", "--proxy", proxy_url])
    processes = []
    output_files = []
    print(f"{datetime.now().time()} | Running command '{args}' in parallel {parallel_files}...")
    for i in range(0, parallel_files):
        f = tempfile.NamedTemporaryFile(
            mode='w+', prefix=f'curl-ul-{i}-', suffix='.log', delete=False)
        proc = subprocess.Popen(
            args + [f"{url}/{os.environ['UPLOAD_FILENAME']}"],
            stdout=f, stderr=subprocess.STDOUT)
        processes.append(proc)
        output_files.append(f)

    speeds_MBps = []
    errors = []
    for i, p in enumerate(processes):
        p.wait()

        output_files[i].close()
        with open(output_files[i].name, 'r') as log:
            output = log.read()

        if p.returncode != 0:
            print(f"{datetime.now().time()} | Transfer failed:\n{output}")
            errors.append(output)
            continue

        speed_str = output.splitlines()[-1].split()[7]
        if 'k' in speed_str:
            speeds_MBps.append(float(speed_str.replace('k', '')) / 1024)
        elif 'M' in speed_str:
            speeds_MBps.append(float(speed_str.replace('M', '')))
        elif 'G' in speed_str:
            speeds_MBps.append(1024 * float(speed_str.replace('G', '')))
        else:
            speeds_MBps.append(float(speed_str) / (1024 * 1024))

    for f in output_files:
        try:
            os.unlink(f.name)
        except OSError:
            pass

    print(f"{datetime.now().time()} | File upload test is done")
    return HttpMeasurements(
        parallel_files,
        speeds_MBps,
        errors,
    )

RESULTS = {}

socks5_proxies = []
if ARGS.proxy and ARGS.socks_ports_range:
    no_scheme = ARGS.proxy.removeprefix("socks5://").removeprefix("socks5h://")
    if len(ARGS.proxy) != len(no_scheme):
        if (']' in no_scheme and not no_scheme.endswith(']')) or ':' in no_scheme:
            print(f"{datetime.now().time()} | "
                  f"SOCKS5 proxy url must not contain port in case ports range is specified")
            sys.exit(1)

    for port in range(ARGS.socks_ports_range[0], ARGS.socks_ports_range[1] + 1):
        socks5_proxies.append(f"{ARGS.proxy}:{port}")


class ThreadReturningValue(threading.Thread):

    def __init__(self, group=None, target=None, name=None, args=(), kwargs=None):
        threading.Thread.__init__(self, group, target, name, args, kwargs)
        self._return = None

    def run(self):
        if self._target is not None:
            self._return = self._target(*self._args, **self._kwargs)

    def join(self, *args):
        threading.Thread.join(self, *args)
        return self._return


if ARGS.download:
    if len(socks5_proxies) > 0:
        threads = list(map(
            lambda proxy: ThreadReturningValue(target=run_file_download_test, args=(ARGS.download, ARGS.jobs, ARGS.proto, proxy)),
            socks5_proxies))
        for t in threads:
            t.start()

        speed_samples = []
        errors = []
        for x in map(lambda x: x.join(), threads):
            speed_samples.extend([x.avg_speed_MBps] * (x.jobs_num - x.failed_num))
            errors.extend(x.errors)
        r = HttpMeasurements(jobs_num=len(socks5_proxies) * ARGS.jobs, speed_samples=speed_samples, errors=errors)
    else:
        r = run_file_download_test(ARGS.download, ARGS.jobs, ARGS.proto, ARGS.proxy)
    print(f"{datetime.now().time()} | HTTP download test results: {r}")
    RESULTS["http_download"] = r

if ARGS.upload:
    if len(socks5_proxies) > 0:
        threads = list(map(
            lambda proxy: ThreadReturningValue(target=run_file_upload_test, args=(ARGS.upload, ARGS.jobs, ARGS.proto, proxy)),
            socks5_proxies))
        for t in threads:
            t.start()

        speed_samples = []
        errors = []
        for x in map(lambda x: x.join(), threads):
            speed_samples.extend([x.avg_speed_MBps] * (x.jobs_num - x.failed_num))
            errors.extend(x.errors)
        r = HttpMeasurements(jobs_num=len(socks5_proxies) * ARGS.jobs, speed_samples=speed_samples, errors=errors)
    else:
        r = run_file_upload_test(ARGS.upload, ARGS.jobs, ARGS.proto, ARGS.proxy)
    print(f"{datetime.now().time()} | HTTP upload test results: {r}")
    RESULTS["http_upload"] = r

if ARGS.output:
    Path(os.path.dirname(ARGS.output)).mkdir(parents=True, exist_ok=True)
    try:
        os.remove(ARGS.output)
    except OSError:
        pass

    RESULTS = {key: val for key, val in RESULTS.items() if val is not None}
    if len(RESULTS) > 0:
        with open(ARGS.output, "w+") as f:
            json.dump(RESULTS, fp=f, indent=2, default=lambda o: o.__dict__)
