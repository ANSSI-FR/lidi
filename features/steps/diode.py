# implementation of steps for "diode start"

from behave import given, when, then, use_step_matcher
import subprocess
import time
import psutil
import os
from tempfile import TemporaryDirectory

from throttle_fs import ThrottledFSProcess

use_step_matcher("cfparse")

def nice(process_name):
    for proc in psutil.process_iter():
        if process_name in proc.name():
            ps = psutil.Process(proc.pid)
            # must be root
            if os.getuid() == 0:
                ps.nice(-20)
            return 

def start_diode_receive(context, network_behavior):
    if context.quiet:
        stdout = subprocess.DEVNULL
        stderr = subprocess.DEVNULL
    else:
        stdout = subprocess.PIPE
        stderr = subprocess.STDOUT

    diode_receive_command = [f'{context.bin_dir}/diode-receive', '--to-tcp', '127.0.0.1:7000', '--session-expiration-delay', '1']
    if network_behavior:
        diode_receive_command.append('--bind-udp')
        diode_receive_command.append('0.0.0.0:6000')
    else:
        diode_receive_command.append('--bind-udp')
        diode_receive_command.append('0.0.0.0:5000')

    if context.mtu:
        diode_receive_command.append('--udp-mtu')
        diode_receive_command.append(str(context.mtu))
        diode_receive_command.append('--repair-block-size')
        diode_receive_command.append(str(2*context.mtu))

    if context.block_size:
        diode_receive_command.append('--encoding-block-size')
        diode_receive_command.append(str(context.block_size))

    if context.log_config_diode_receive:
        diode_receive_command.append('--log-config')
        diode_receive_command.append(context.log_config_diode_receive)

    context.proc_diode_receive = subprocess.Popen(diode_receive_command, stdout=stdout, stderr=stderr)
    # here we need to wait enough time for diode-receive to be ready
    time.sleep(2)
    poll = context.proc_diode_receive.poll()
    if poll:
        print(context.proc_diode_receive.communicate())
        raise Exception("Can't start diode receive")

    nice('diode-receive')

def stop_diode_receive(context):
    if context.proc_diode_receive:
        context.proc_diode_receive.kill()

def start_diode_send(context):
    if context.quiet:
        stdout = subprocess.DEVNULL
        stderr = subprocess.DEVNULL
    else:
        stdout = subprocess.PIPE
        stderr = subprocess.STDOUT

    diode_send_command = [f'{context.bin_dir}/diode-send', '--bind-tcp', '127.0.0.1:5000', '--to-udp', '127.0.0.1:5000', '--nb-threads', str(context.send_nb_threads)]
    if context.mtu:
        diode_send_command.append('--udp-mtu')
        diode_send_command.append(str(context.mtu))
        diode_send_command.append('--repair-block-size')
        diode_send_command.append(str(2*context.mtu))

    if context.block_size:
        diode_send_command.append('--encoding-block-size')
        diode_send_command.append(str(context.block_size))

    if context.read_rate:
        diode_send_command.append('--max-bandwidth')
        diode_send_command.append(str(context.read_rate))

    if context.log_config_diode_send:
        diode_send_command.append('--log-config')
        diode_send_command.append(context.log_config_diode_send)

    context.proc_diode_send = subprocess.Popen(diode_send_command, stdout=stdout, stderr=stderr)
    time.sleep(0.5)
    poll = context.proc_diode_send.poll()
    if poll:
        print(context.proc_diode_send.communicate())
        raise Exception("Can't start diode send")
    nice('diode-send')

def stop_diode_send(context):
    if context.proc_diode_send:
        context.proc_diode_send.kill()

def start_diode(context):
    if context.quiet:
        stdout = subprocess.DEVNULL
        stderr = subprocess.DEVNULL
    else:
        stdout = subprocess.PIPE
        stderr = subprocess.STDOUT

    network_behavior = False
    network_command = [f'{context.bin_dir}/network-behavior', '--bind-udp', '0.0.0.0:5000', '--to-udp', '127.0.0.1:6000']
    if context.network_down_after:
        network_command.append('--network-down-after')
        network_command.append(str(context.network_down_after))
        network_behavior = True

    if context.network_up_after:
        network_command.append('--network-up-after')
        network_command.append(str(context.network_up_after))
        network_behavior = True

#    if context.network_max_bandwidth:
#        network_command.append('--max-bandwidth')
#        network_command.append(str(context.network_max_bandwidth))
#        network_behavior = True

    if context.network_drop:
        network_command.append('--loss-rate')
        network_command.append(context.network_drop)
        network_behavior = True

    if network_behavior:
        context.proc_network = subprocess.Popen(network_command)
        time.sleep(1)

    # start diode-receive-file (tcp server)
    diode_receive_file_command = [f'{context.bin_dir}/diode-receive-file', '--bind-tcp', '127.0.0.1:7000', context.receive_dir.name]
    if context.log_config_diode_receive_file:
        diode_receive_file_command.append('--log-config')
        diode_receive_file_command.append(context.log_config_diode_receive_file)

    context.proc_diode_receive_file = subprocess.Popen(
        diode_receive_file_command,
        stdout=stdout, stderr=stderr)

    time.sleep(1)

    # start diode-receive (connects to diode-receive-file)
    start_diode_receive(context, network_behavior)

    # finally start diode-send (send init packet to diode-receive, acts as a server for diode-send-file)
    start_diode_send(context)


def start_throttled_diode(context, read_rate):
    context.send_ratelimit_dir = TemporaryDirectory()

    context.proc_throttled_fs = ThrottledFSProcess(context.send_ratelimit_dir.name, context.send_dir.name, read_rate)
    context.proc_throttled_fs.start()

    time.sleep(1)

    start_diode(context)

@given('diode is started')
def step_impl(context):
    start_diode(context)

@when('diode-receive is restarted')
def step_impl(context):
    stop_diode_receive(context)
    # wait some time to prevent address already in use if restarted too quickly
    time.sleep(5)
    start_diode_receive(context, False)

@when('diode-send is restarted')
def step_impl(context):
    stop_diode_send(context)
    start_diode_send(context)

@given('diode is started with max throughput of {throughput} Mb/s')
def step_diode_started_with_max_throughput(context, throughput):
    # two possibilities : limit file system read throughput or configure the diode for that
    context.read_rate = int(throughput) * 1000000
    start_throttled_diode(context, int(context.read_rate / 8))

@given('diode is started with max throughput of {throughput} Mb/s and MTU {mtu}')
def step_diode_started_with_max_throughput(context, throughput, mtu):
    # two possibilities : limit file system read throughput or configure the diode for that
    context.read_rate = int(throughput) * 1000000
    context.mtu = int(mtu)
    start_throttled_diode(context, int(context.read_rate / 8))
