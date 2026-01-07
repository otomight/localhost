#!/usr/bin/env python3
import os
import sys

UPLOAD_DIR = "../data/uploads"
os.makedirs(UPLOAD_DIR, exist_ok=True)

content_type = os.environ.get("CONTENT_TYPE", "")
content_length = int(os.environ.get("CONTENT_LENGTH", "0"))

if "multipart/form-data" not in content_type:
	print("Status: 400 Bad Request")
	print()
	print("Invalid content type")
	sys.exit(0)

boundary = content_type.split("boundary=")[1]
boundary = ("--" + boundary).encode()

data = sys.stdin.buffer.read(content_length)

parts = data.split(boundary)
for part in parts:
	if b"Content-Disposition" not in part:
		continue

	headers, body = part.split(b"\r\n\r\n", 1)
	body = body.rstrip(b"\r\n--")

	for line in headers.split(b"\r\n"):
		if b"filename=" in line:
			filename = line.split(b"filename=")[1].strip(b'"')
			filepath = os.path.join(UPLOAD_DIR, filename.decode())

			with open(filepath, "wb") as f:
				f.write(body)

			print("Status: 200 OK")
			print("Content-Type: text/plain")
			print()
			print(f"Uploaded {filename.decode()}")
			sys.exit(0)

print("Status: 400 Bad Request")
print()
print("No file received")
