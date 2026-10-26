#!/usr/bin/env python3
"""Pandoc normal-filter shim for upstream's nonstandard --source stage."""
import os, pathlib, subprocess, sys
if len(sys.argv)!=2 or sys.argv[1] not in ('json','html'):
    raise SystemExit('expected Pandoc writer target json or html')
source=pathlib.Path(os.environ['TANGLEWOOD_P0_SOURCE'])
result=subprocess.run([sys.executable,'-m','illustration_md.prepare','--source',str(source)],input=sys.stdin.buffer.read(),capture_output=True)
sys.stderr.buffer.write(result.stderr)
if result.returncode: raise SystemExit(result.returncode)
pathlib.Path(os.environ['TANGLEWOOD_P0_OUTPUT']+'.'+sys.argv[1]+'.prepared.json').write_bytes(result.stdout)
sys.stdout.buffer.write(result.stdout)
