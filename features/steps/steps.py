
from behave import given, when, then, use_step_matcher
import time

from features.steps.diode import create_file, send_file, send_multiple_files, start_diode, start_diode_file_receive, start_diode_receive, start_diode_send, start_diode_send_dir, start_throttled_diode, stop_diode_file_receive, stop_diode_receive, stop_diode_send
from features.steps.file import create_and_copy_file, create_and_copy_multiple_files, create_and_move_file, parse_human_size, test_file, test_no_file

use_step_matcher("cfparse")

@given('diode is started')
def step_impl(context):
    start_diode(context)

@when('diode-receive is restarted')
def step_impl(context):
    stop_diode_receive(context)
    # wait some time to prevent address already in use if restarted too quickly
    time.sleep(5)
    start_diode_receive(context)

@when('diode-send is restarted')
def step_impl(context):
    stop_diode_send(context)
    start_diode_send(context)

@when('diode-file-receive is restarted')
def step_impl(context):
    stop_diode_file_receive(context)
    # wait some time to prevent address already in use if restarted too quickly
    time.sleep(5)
    start_diode_file_receive(context)

@when('diode-send-dir is started')
def step_impl(context):
    start_diode_send_dir(context)

@given('diode is started with max throughput of {throughput} Mb/s')
def step_diode_started_with_max_throughput(context, throughput):
    # two possibilities : limit file system read throughput or configure the diode for that
    context.read_rate = int(throughput)
    start_throttled_diode(context, int(context.read_rate * 1000000 / 8))

@given('diode is started with max throughput of {throughput} Mb/s and MTU {mtu}')
def step_diode_started_with_max_throughput(context, throughput, mtu):
    # two possibilities : limit file system read throughput or configure the diode for that
    context.read_rate = int(throughput)
    context.mtu = int(mtu)
    start_throttled_diode(context, int(context.read_rate * 1000000 / 8))

@given('encoding block size is {encoding} and repair block size is {repair}')
def step_set_encoding_repair_block_size(context, encoding, repair):
    context.repair_block = 20000
    context.block_size = 20000

@when('diode-file-send file {name} of size {size}')
def step_impl(context, name, size):
    send_file(context, name, size)

@when('diode-send restarts while diode-file-send file {name} of size {size}')
def step_impl(context, name, size):
    send_file(context, name, size, True)
    # transfer is in progress, wait 1 second then restart diode
    time.sleep(3)
    stop_diode_send(context)
    start_diode_send(context)

@when('diode-receive restarts while diode-file-send file {name} of size {size}')
def step_impl(context, name, size):
    send_file(context, name, size, True)
    # transfer is in progress, wait 1 second then restart diode
    time.sleep(3)
    stop_diode_receive(context)
    time.sleep(5)
    start_diode_receive(context)

@then('diode-file-receive file {name} in {seconds} seconds')
def step_impl(context, name, seconds):
    test_file(context, context.receive_dir.name, name, seconds)

@when('diode-file-receive file {name} in {seconds} seconds')
def step_impl(context, name, seconds):
    test_file(context, context.receive_dir.name, name, seconds)

@when('diode-file-send {files} files of size {size}')
def step_impl(context, files, size):
    for i in range(int(files)):
        name = str(f"test_file_{i}")
        create_file(context, name, size)

    # now send all of them at once
    send_multiple_files(context)

@then('diode-file-receive all files in {seconds} seconds')
def step_impl(context, seconds):
    for name in context.files:
        test_file(context, context.receive_dir.name, name, seconds)

@given(u'diode with send-dir is started')
def step_impl(context):
    start_diode(context)
    start_diode_send_dir(context)

@when(u'we copy a file {name} of size {size}')
def step_impl(context, name, size):
    create_and_copy_file(context, name, size)

@when(u'we copy {files} files of size {size}')
def step_impl(context, files, size):
    create_and_copy_multiple_files(context, files, size)

@when(u'we move a file {name} of size {size}')
def step_impl(context, name, size):
    create_and_move_file(context, name, size)

@then('diode-file-receive no file {name} in {seconds} seconds')
def step_impl(context, name, seconds):
    test_no_file(context, context.receive_dir.name, name, seconds)

@then(u'file {name} is in source directory')
def step_impl(context, name):
    test_file(context, context.send_dir.name, name, 1)

@given('there is a network interrupt of {network_up_after} after {network_down_after}')
def step_impl(context, network_up_after, network_down_after):
    context.network_down_after = parse_human_size(network_down_after)
    context.network_up_after = parse_human_size(network_up_after) + context.network_down_after

@given('there is a network drop of {percent} %')
def step_given_network_drop_percent(context, percent):
    context.network_drop = percent

@given('there is a limited network bandwidth of {bandwidth} Mb/s')
def step_given_network_limited_bandwidth(context, bandwidth):
    context.network_max_bandwidth = str(int(bandwidth) * 1000000)

@given('network bandwidth must not exceed {bandwidth} Mb/s')
def step_limited_bandwidth_not_exceeded(context, bandwidth):
    context.bandwidth_must_not_exceed = str(int(bandwidth) * 1000000)
