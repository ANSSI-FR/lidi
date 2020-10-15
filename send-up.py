#!/usr/bin/env python3

import array
import cbor
import socket
import sys

queue = sys.argv[1]
filename = sys.argv[2]

sock = socket.socket(family=socket.AF_UNIX)
sock.connect("/run/lidi-down.socket")

with open(filename) as f:
    message = { "queue": queue, "metadata": None }

    sock.sendmsg(
        [cbor.dumps(message)],
        [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", [f.fileno()]))]
    )
