#!/bin/bash
set -e

SERVER="http://localhost:8080"
FILE="../files/light_upload.txt"

echo "[TEST] CGI upload"

curl -s -o /dev/null -w "%{http_code}" \
  -F "file=@${FILE}" \
  "${SERVER}/upload" | grep -q 200

echo "[OK] upload passed"
