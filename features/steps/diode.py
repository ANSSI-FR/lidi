# implementation of steps for "diode start"

from behave import given, when, then, use_step_matcher
import subprocess
import time
from tempfile import TemporaryDirectory

from throttle_fs import ThrottledFSProcess

use_step_matcher("cfparse")

def start_diode(context):
    network_command = ['cargo', 'run', '--release', '--bin', 'network-behavior', '--', '--from-udp', '0.0.0.0:5000', '--to-udp', '127.0.0.1:6000']
    if context.network_down_after:
        network_command.append('--network-down-after')
        network_command.append(str(context.network_down_after))

    if context.network_up_after:
        network_command.append('--network-up-after')
        network_command.append(str(context.network_up_after))

    if context.network_max_bandwidth:
        network_command.append('--max-bandwidth')
        network_command.append(str(context.network_max_bandwidth))

    if context.network_drop:
        network_command.append('--loss-rate')
        network_command.append(context.network_drop)

    context.proc_network = subprocess.Popen(
        network_command
    )
    time.sleep(0.1)
    context.proc_diode_receive = subprocess.Popen(
        ['cargo', 'run', '--release', '--bin', 'diode-receive', '--', '--from_udp', '0.0.0.0:6000', '--to_tcp', '0.0.0.0:7000']
    )
    time.sleep(0.1)
    context.proc_diode_send = subprocess.Popen(
        ['cargo', 'run', '--release', '--bin', 'diode-send', '--', '--to_udp', '127.0.0.1:5000']
    )
    time.sleep(0.1)
    context.proc_diode_receive_file = subprocess.Popen(
        ['cargo', 'run', '--release', '--bin', 'diode-receive-file', '--', context.receive_dir.name]
    )

    time.sleep(1)

@given('diode is started')
def step_impl(context):
    start_diode(context)

@given('diode is started with max throughput of {throughput} Mb/s')
def step_diode_started_with_max_throughput(context, throughput):
    # two possibilities : limit file system read throughput or configure the diode for that
    read_rate = int(throughput) * 1000000 / 8
    context.send_ratelimit_dir = TemporaryDirectory()

    context.proc_throttled_fs = ThrottledFSProcess(context.send_ratelimit_dir.name, context.send_dir.name, read_rate)
    context.proc_throttled_fs.start()

    time.sleep(1)

    start_diode(context)
