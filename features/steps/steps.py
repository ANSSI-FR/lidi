
from behave import given, when, then, use_step_matcher
import os
import time

from features.steps.lidi import create_file, send_file, send_multiple_files, start_diode, start_lidi_file_receive, start_lidi_receive, start_lidi_send, start_lidi_send_dir, start_throttled_diode, stop_lidi_file_receive, stop_lidi_receive, stop_lidi_send
from features.steps.file import create_and_copy_file, create_and_copy_multiple_files, create_and_move_file, parse_human_size, test_file, test_no_file

use_step_matcher("cfparse")

@given('lidi is started')
def step_impl(context):
    start_diode(context)

@when('lidi-receive is restarted')
def step_impl(context):
    stop_lidi_receive(context)
    # wait some time to prevent address already in use if restarted too quickly
    time.sleep(5)
    start_lidi_receive(context)

@when('lidi-send is restarted')
def step_impl(context):
    stop_lidi_send(context)
    # wait for lidi-receive reset timeout to happen
    time.sleep(2)
    start_lidi_send(context)

@when('lidi-file-receive is restarted')
def step_impl(context):
    stop_lidi_file_receive(context)
    # wait some time to prevent address already in use if restarted too quickly
    time.sleep(5)
    start_lidi_file_receive(context)

@given('lidi-dir-send is started with watch and ignore dot files')
def step_lidi_send_dir_with_watch_and_ignore_dot_files(context):
    start_lidi_send_dir(context, True, '^\\.')

@given('lidi-dir-send is started with watch')
def step_lidi_send_dir_with_watch(context):
    start_lidi_send_dir(context, True)

@given('lidi-dir-send is started')
def step_lidi_send_dir(context):
    start_lidi_send_dir(context)

@given('lidi is started with max throughput of {throughput} and MTU {mtu}')
def step_lidi_started_with_max_throughput_and_mtu(context, throughput, mtu):
    # throughput format: tc notation (e.g., "100mbit", "990kbit")
    # mtu: maximum transmission unit in bytes
    context.read_rate = throughput
    context.mtu = int(mtu)
    start_throttled_diode(context, context.read_rate, int(mtu))

@given('lidi is started with max throughput of {throughput}')
def step_lidi_started_with_max_throughput(context, throughput):
    # two possibilities : limit file system read throughput or configure the lidi for that
    # throughput format: tc notation (e.g., "100mbit", "990kbit")
    context.read_rate = throughput
    start_throttled_diode(context, context.read_rate)

@given('encoding block size is {encoding}')
def step_set_encoding(context, encoding):
    context.block_size = encoding

@given('repair percentage is {repair} %')
def step_set_encoding(context, repair):
    context.repair = repair
    
@when('lidi-file-send file {name} of size {size}')
def step_impl(context, name, size):
    send_file(context, name, size)

@when('lidi-send restarts while lidi-file-send file {name} of size {size}')
def step_impl(context, name, size):
    send_file(context, name, size, True)
    # transfer is in progress, wait 1 second then restart diode
    time.sleep(3)
    stop_lidi_send(context)
    start_lidi_send(context)

@when('lidi-receive restarts while lidi-file-send file {name} of size {size}')
def step_impl(context, name, size):
    send_file(context, name, size, True)
    # transfer is in progress, wait 1 second then restart diode
    time.sleep(3)
    stop_lidi_receive(context)
    time.sleep(5)
    start_lidi_receive(context)

@then('lidi-file-receive file {name} in {seconds} seconds')
def step_impl(context, name, seconds):
    test_file(context, context.receive_dir, name, seconds)

@when('lidi-file-receive file {name} in {seconds} seconds')
def step_impl(context, name, seconds):
    test_file(context, context.receive_dir, name, seconds)

@when('lidi-file-send {files} files of size {size}')
def step_impl(context, files, size):
    for i in range(int(files)):
        name = str(f"test_file_{i}")
        filename = os.path.join(context.send_dir, name)
        create_file(context, filename, size)

    # now send all of them at once
    send_multiple_files(context)

@then('lidi-file-receive all files in {seconds} seconds')
def step_impl(context, seconds):
    for name in context.files:
        test_file(context, context.receive_dir, name, seconds)

@given(u'lidi with send-dir is started')
def step_impl(context):
    start_diode(context)
    start_lidi_send_dir(context)

@when(u'we copy a file {name} of size {size}')
def step_impl(context, name, size):
    create_and_copy_file(context, name, size)

@when(u'we copy {files} files of size {size}')
def step_impl(context, files, size):
    create_and_copy_multiple_files(context, files, size)

@when(u'we move a file {name} of size {size}')
def step_impl(context, name, size):
    create_and_move_file(context, name, size)

@then('lidi-file-receive no file {name} in {seconds} seconds')
def step_impl(context, name, seconds):
    test_no_file(context, context.receive_dir, name, seconds)

@then(u'file {name} is in source directory')
def step_impl(context, name):
    test_file(context, context.send_dir, name, 1)

@given('there is a network interrupt of {network_up_after} after {network_down_after}')
def step_impl(context, network_up_after, network_down_after):
    context.network_down_after = parse_human_size(network_down_after)
    context.network_up_after = parse_human_size(network_up_after) + context.network_down_after

@given('there is a network drop of {percent} %')
def step_given_network_drop_percent(context, percent):
    context.network_drop = percent

@given('there is a limited network bandwidth of {bandwidth} Mb/s')
def step_given_network_limited_bandwidth(context, bandwidth):
    # used by network simulator to drop packets if bandwidth is higher than that
    context.network_max_bandwidth = str(int(bandwidth) * 1000000)

@given('network bandwidth must not exceed {bandwidth} Mb/s')
def step_limited_bandwidth_not_exceeded(context, bandwidth):
    # used by network simulator to abort if received bandwidth is higher than that
    context.bandwidth_must_not_exceed = str(int(bandwidth) * 1000000)
