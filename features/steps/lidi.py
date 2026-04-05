# implementation of steps for "lidi start"
#
# This code start the following applications on the following ports :
#
#  [lidi-file-send]   -->   [lidi-send]   -->   [lidi-receive]   <--   [lidi-file-receive]
#             TCP tcp_send_port         UDP 6000            TCP tcp_receive_port
#
# Or, when using lidi-network-simulator :
#
#  [lidi-file-send]   -->   [lidi-send]   -->   [lidi-network-simulator]   -->   [lidi-receive]   <--   [lidi-file-receive]
#             TCP tcp_send_port         UDP 5000                         UDP 6000           TCP tcp_receive_port
# 
#  IP/PORT Configuration:
#  - lidi-dir-send: TCP server on 127.0.0.1:tcp_send_port
#  - lidi-send: UDP client on 127.0.0.1:5000 (or 6000 if network behavior), TCP server on 127.0.0.1:tcp_send_port
#  - lidi-receive: UDP server on 127.0.0.1:5000 (or 6000 if network behavior), TCP server on 127.0.0.1:tcp_receive_port
#  - lidi-file-receive: TCP client on 127.0.0.1:tcp_receive_port
#  - lidi-network-simulator (if used):
#      - UDP bind on 0.0.0.0:5000
#      - UDP to 127.0.0.1:6000
#
#  Network Behavior (when enabled):
#  - lidi-network-simulator handles simulated network behavior
#  - UDP traffic flows from lidi-send (5000) to lidi-network-simulator (5000)
#  - lidi-network-simulator forwards to lidi-receive (6000)
#  - This enables testing of network conditions like bandwidth limitations, packet loss, etc.

import os
import psutil
import subprocess
import time
from contextlib import contextmanager

from features.steps.config import build_lidi_send_dir_command, build_lidi_send_file_command, build_lidi_receive_command, build_lidi_receive_file_command, build_lidi_send_command, build_network_simulator_command, write_lidi_config
from features.steps.file import create_file
from features.steps.tc_shaper import TcUdpShaper
from features.steps.utils import stop_process, nice, PROCESS_READY_DELAY, PROCESS_READY_DELAY_EXTENDED
        
def start_lidi_receive(context):
    """Start the lidi receive process."""
    lidi_receive_command = build_lidi_receive_command(context)

    context.proc_lidi_receive = subprocess.Popen(
        lidi_receive_command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )
    
    # Wait enough time for lidi-receive to be ready
    time.sleep(PROCESS_READY_DELAY_EXTENDED)

    # Check it is running
    poll = context.proc_lidi_receive.poll()
    if poll is not None:
        stdout, stderr = context.proc_lidi_receive.communicate()
        print(f"lidi-receive failed with return code {poll}")
        print(f"Stdout: {stdout}")
        print(f"Stderr: {stderr}")
        raise Exception("Can't start lidi receive")

    nice('lidi-receive')

def stop_lidi_receive(context):
    """Stop the lidi receive process."""
    stop_process(context, 'proc_lidi_receive')

def start_lidi_file_receive(context):
    """Start the lidi receive file process."""
    lidi_receive_file_command = build_lidi_receive_file_command(context)

    # Start lidi-file-receive
    context.proc_lidi_receive_file = subprocess.Popen(lidi_receive_file_command)

def stop_lidi_file_receive(context):
    """Stop the lidi file receive process."""
    stop_process(context, 'proc_lidi_receive_file')

def start_lidi_send(context):
    """Start the lidi send process."""
    lidi_send_command = build_lidi_send_command(context)

    # Start lidi-send
    context.proc_lidi_send = subprocess.Popen(lidi_send_command)

    # Wait enough time for lidi-send to be ready
    time.sleep(PROCESS_READY_DELAY)

    # Check it is running
    poll = context.proc_lidi_send.poll()
    if poll is not None:
        stdout, stderr = context.proc_lidi_send.communicate()
        print(f"lidi-send failed with return code {poll}")
        print(f"Stdout: {stdout}")
        print(f"Stderr: {stderr}")
        raise Exception("Can't start lidi send")
    nice('lidi-send')

def stop_lidi_send(context):
    """Stop the lidi send process."""
    if context.proc_lidi_send:
        context.proc_lidi_send.kill()

def start_diode(context):
    """Start the complete lidi system with network simulation if needed."""
    network_simulator_command = build_network_simulator_command(context)

    # Start network simulator if behavior is configured
    if network_simulator_command:
        context.proc_network = subprocess.Popen(network_simulator_command)
        time.sleep(PROCESS_READY_DELAY)

    # Start lidi receive file process
    start_lidi_file_receive(context)
    time.sleep(PROCESS_READY_DELAY)

    # Start lidi receive (connects to lidi-file-receive)
    start_lidi_receive(context)

    # Finally start lidi send (send init packet to lidi-receive, acts as a server for lidi-file-send)
    start_lidi_send(context)


def start_throttled_diode(context, read_rate: str, mtu: int | None = None):
    """Start lidi with tc-based UDP bandwidth shaping on loopback."""
    # read_rate : notation tc, ex. "10mbit", "500kbit"
    # mtu : maximum transmission unit in bytes (optional)
    if mtu:
        context.mtu = mtu
    context.tc_shaper = TcUdpShaper(rate=read_rate, port=5000)
    context.tc_shaper.setup()

    start_diode(context)

def stop_throttled_diode(context):
    """Teardown tc shaping if active."""
    if hasattr(context, 'tc_shaper') and context.tc_shaper:
        context.tc_shaper.teardown()
        context.tc_shaper = None

def start_lidi_send_dir(context, watch=False, ignore=None):
    """Start the lidi send directory process."""
    lidi_send_dir_command = build_lidi_send_dir_command(context, watch, ignore)

    # Start lidi-dir-send
    context.proc_lidi_send_dir = subprocess.Popen(lidi_send_dir_command)

    time.sleep(PROCESS_READY_DELAY)

def send_file_command(context, filename, background=False):
    """Execute send file command with specified parameters."""    
    lidi_send_file_command = build_lidi_send_file_command(context, filename)

    if not background:
        # Execute the command
        result = subprocess.run(
            lidi_send_file_command,
            timeout=300,
            text=True
        )
        if result.returncode != 0:
            print(f"DEBUG: send_file_command failed: {result.stderr}")
        result.check_returncode()
    else:
        # For background mode, we also need to capture output
        context.proc_lidi_send_file = subprocess.Popen(lidi_send_file_command)
        # No assert needed here, Popen always returns a valid object

def send_file(context, name, size, background=False):
    """Send a file with specified name and size."""
    # Create file in send directory
    filename = os.path.join(context.send_dir, name)
    create_file(context, filename, size)

    # Send it (using buffer size of 8192 to limit bursts & packet drops)
    send_file_command(context, filename, background)

def send_multiple_files(context):
    """Send multiple files from context."""
    # Send all files - use full paths from context.files
    file_paths = [context.files[name]['path'] for name in context.files.keys()]
    send_file_command(context, file_paths, background=False)

