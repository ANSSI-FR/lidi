# implementation of steps for "diode start"
#
# This code start the following applications on the following ports :
#
#  [lidi-send-file]   -->   [lidi-send]   -->   [lidi-receive]   <--   [lidi-receive-file]
#             TCP tcp_send_port         UDP 6000            TCP tcp_receive_port
#
# Or, when using lidi-network-simulator :
#
#  [lidi-send-file]   -->   [lidi-send]   -->   [lidi-network-simulator]   -->   [lidi-receive]   <--   [lidi-receive-file]
#             TCP tcp_send_port         UDP 5000                         UDP 6000           TCP tcp_receive_port
# 
#  IP/PORT Configuration:
#  - lidi-send-dir: TCP server on 127.0.0.1:tcp_send_port
#  - lidi-send: UDP client on 127.0.0.1:5000 (or 6000 if network behavior), TCP server on 127.0.0.1:tcp_send_port
#  - lidi-receive: UDP server on 127.0.0.1:5000 (or 6000 if network behavior), TCP server on 127.0.0.1:tcp_receive_port
#  - lidi-receive-file: TCP client on 127.0.0.1:tcp_receive_port
#  - lidi-network-simulator (if used):
#      - UDP bind on 0.0.0.0:5000
#      - UDP to 127.0.0.1:6000
#
#  Network Behavior (when enabled):
#  - lidi-network-simulator handles simulated network behavior
#  - UDP traffic flows from lidi-send (5000) to lidi-network-simulator (5000)
#  - lidi-network-simulator forwards to lidi-receive (6000)
#  - This enables testing of network conditions like bandwidth limitations, packet loss, etc.

import hashlib
import os
import psutil
import shutil
import subprocess
import time
from tempfile import TemporaryDirectory
from contextlib import contextmanager

from features.steps.config import log_files, write_lidi_config
from features.steps.file import create_file
from features.steps.throttle_fs import ThrottledFSProcess

def stop_process(context, process_attr):
    """Stop a process if it exists."""
    if hasattr(context, process_attr):
        process = getattr(context, process_attr)
        if process:
            try:
                process.kill()
            except Exception:
                # Process might have already terminated
                pass

def nice(process_name):
    """Set process priority (niceness) if running as root."""
    for proc in psutil.process_iter():
        if process_name in proc.name():
            process = psutil.Process(proc.pid)
            # must be root
            if os.getuid() == 0:
                process.nice(-20)
            return
        
def start_diode_receive(context):
    """Start the diode receive process."""
    # Determine UDP port based on network behavior
    has_network_simulator = (
        context.network_down_after or
        context.network_up_after or
        context.network_drop or
        context.network_max_bandwidth or
        context.bandwidth_must_not_exceed
    )
    receiver_bind_udp_port = "6000" if has_network_simulator else "5000"

    lidi_config = write_lidi_config(context, "lidi_receive.toml", receiver_bind_udp_port, context.log_config_diode_receive)

    diode_receive_command = [f'{context.bin_dir}/lidi-receive', lidi_config]

    context.proc_diode_receive = subprocess.Popen(
        diode_receive_command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )
    
    # Wait enough time for diode-receive to be ready
    time.sleep(2)
    poll = context.proc_diode_receive.poll()
    if poll is not None:
        stdout, stderr = context.proc_diode_receive.communicate()
        print(f"diode-receive failed with return code {poll}")
        print(f"Stdout: {stdout}")
        print(f"Stderr: {stderr}")
        raise Exception("Can't start diode receive")

    nice('diode-receive')

def start_diode_file_receive(context):
    """Start the diode receive file process."""
    # Start diode-receive-file (TCP server)
    diode_receive_file_command = [
        f'{context.bin_dir}/lidi-receive-file',
        '--from-tcp',
        f'127.0.0.1:{context.tcp_receive_port}',
        context.receive_dir.name
    ]

    with log_files(context.base_dir, 'receive-file') as (stdout, stderr):
        context.proc_diode_receive_file = subprocess.Popen(
            diode_receive_file_command,
            stdout=stdout,
            stderr=stderr
        )

def stop_diode_receive(context):
    """Stop the diode receive process."""
    stop_process(context, 'proc_diode_receive')

def stop_diode_file_receive(context):
    """Stop the diode file receive process."""
    stop_process(context, 'proc_diode_receive_file')

def start_diode_send(context):
    """Start the diode send process."""
    lidi_config = write_lidi_config(context, "lidi_send.toml", "5000", context.log_config_diode_send)

    diode_send_command = [f'{context.bin_dir}/lidi-send', lidi_config]

    context.proc_diode_send = subprocess.Popen(
        diode_send_command,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL
    )
    time.sleep(0.5)
    poll = context.proc_diode_send.poll()
    if poll is not None:
        stdout, stderr = context.proc_diode_send.communicate()
        print(f"diode-send failed with return code {poll}")
        print(f"Stdout: {stdout}")
        print(f"Stderr: {stderr}")
        raise Exception("Can't start diode send")
    nice('diode-send')

def stop_diode_send(context):
    """Stop the diode send process."""
    if context.proc_diode_send:
        context.proc_diode_send.kill()

def start_diode(context):
    """Start the complete diode system with network simulation if needed."""
    # Setup network behavior parameters
    network_command = [
        f'{context.bin_dir}/lidi-network-simulator',
        '--bind-udp', '0.0.0.0:5000',
        '--to-udp', '127.0.0.1:6000',
        '--log-config', context.log_config_network_behavior
    ]
    
    # Add network behavior options
    network_options = [
        ('network_down_after', '--network-down-after'),
        ('network_up_after', '--network-up-after'),
        ('network_drop', '--loss-rate'),
        ('network_max_bandwidth', '--max-bandwidth'),
        ('bandwidth_must_not_exceed', '--abort-on-max-bandwidth')
    ]
    
    network_behavior = False
    for attr_name, option in network_options:
        attr_value = getattr(context, attr_name, None)
        if attr_value:
            network_command.extend([option, str(attr_value)])
            network_behavior = True

    # Start network simulator if behavior is configured
    if network_behavior:
        context.proc_network = subprocess.Popen(network_command)
        time.sleep(1)

    # Start diode receive file process
    start_diode_file_receive(context)
    time.sleep(1)

    # Start diode receive (connects to diode-receive-file)
    start_diode_receive(context)

    # Finally start diode send (send init packet to diode-receive, acts as a server for diode-send-file)
    start_diode_send(context)


def start_throttled_diode(context, read_rate):
    """Start diode with throttled filesystem."""
    context.send_ratelimit_dir = TemporaryDirectory()

    context.proc_throttled_fs = ThrottledFSProcess(context.send_ratelimit_dir.name, context.send_dir.name, read_rate)
    context.proc_throttled_fs.start()

    time.sleep(1)

    start_diode(context)

def start_diode_send_dir(context):
    """Start the diode send directory process."""

    diode_send_dir_command = [
        f'{context.bin_dir}/lidi-send-dir',
        '--max-files', '1',
        '--to-tcp', f'127.0.0.1:{context.tcp_send_port}',
        context.send_dir.name
    ]

    with log_files(context.base_dir, 'send-dir') as (stdout, stderr):
        context.proc_diode_send_dir = subprocess.Popen(
            diode_send_dir_command,
            stdout=stdout,
            stderr=stderr
        )

    time.sleep(1)

def send_file_command(context, filename, background=False):
    """Execute send file command with specified parameters."""    
    cmd_args = [
        f"{context.bin_dir}/lidi-send-file",
        "--buffer-size",
        "8192",
        "--to-tcp",
        f"127.0.0.1:{context.tcp_send_port}",
        filename
    ]

    if not background:
        # Execute the command
        with log_files(context.base_dir, 'send-file') as (stdout, stderr):
            result = subprocess.run(
                cmd_args,
                stdout=stdout,
                stderr=stderr,
                timeout=60,
                text=True
            )
            result.check_returncode()
    else:
        # For background mode, we also need to capture output
        with log_files(context.base_dir, 'send-file') as (stdout, stderr):
            context.proc_diode_send_file = subprocess.Popen(
                cmd_args,
                stdout=stdout,
                stderr=stderr)
            # No assert needed here, Popen always returns a valid object

def send_file(context, name, size, background=False):
    """Send a file with specified name and size."""
    # Create file in send directory
    filename = os.path.join(context.send_dir.name, name)
    create_file(context, filename, size)

    # Take care of possible throttled fs to limit tx throughput
    if context.send_ratelimit_dir:
        filename = os.path.join(context.send_ratelimit_dir.name, name)

    # Send it (using buffer size of 8192 to limit bursts & packet drops)
    send_file_command(context, filename, background)

def send_multiple_files(context):
    """Send multiple files from context."""
    # Build list of filenames
    files = " ".join(context.files.keys())
    
    # Send it (using buffer size of 8192 to limit bursts & packet drops)
    send_file_command(context, files, background=False)

