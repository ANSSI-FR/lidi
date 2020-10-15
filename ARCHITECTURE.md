# Architecture

This document describes the overall architecture of the diode counter.

## Architecture of the DOWN side

The down side is a multi-process program: it consists of a master process that
spawns worker processes.

The master process is in charge of watching over a 'staging' directory using
inotify. Each time a file is moved into that directory, the master process will
open that file and then send it to the next worker.

Each worker process polls a socket, waiting for a new file from the master
process. Once a new file is received, it is queued up in a FIFO and each file is
sent in sequence.

For now, the master process distributes jobs to workers in a round-robin
fashion. TODO: add additional job distribution algorithms like least jobs.

## Architecture of the UP side

The up side is a multi-process program: it consists of a master process that
spawns worker processes.

The master process binds onto the address/port provided and receives datagrams
from the network. Each time a datagram for a new file is received, a worker is
selected and the datagram is forwarded to that worker. Each subsequent datagram
for that file will be forwarded to the same worker.

Each worker polls a socket, waiting for datagrams from the master process. That
datagram is then processed. If the transfers completes or fails, the master is
notified.
