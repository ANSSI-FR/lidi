import argparse
import sys
import os

sys.path.append('steps')

from tempfile import TemporaryDirectory
from environment import before_scenario, after_scenario, setup_log_config
from diode import start_throttled_diode
from diode_file import send_file, receive_file
import time


class Context:
    def __init__(self):
        # test temp dir
        self.send_dir = TemporaryDirectory()
        self.send_ratelimit_dir = None
        self.receive_dir = TemporaryDirectory()

        # files metadata
        self.files = {}

        # process instances
        self.proc_diode_receive = None
        self.proc_diode_send = None
        self.proc_network = None
        self.proc_diode_receive_file = None
        self.proc_throttled_fs = None

        # some possible options
        self.network_down_after = None
        self.network_up_after = None
        self.network_max_bandwidth = None
        self.network_drop = None


context = Context()
context.mtu = 9000
context.block_size = 300000
context.send_nb_threads = 4
context.quiet = True

file_count = 1000
file_size = 1
bandwidth = 1000 # 1000 Mbit/s

parser = argparse.ArgumentParser(
                    prog='ProgramName',
                    description='What the program does',
                    epilog='Text at the bottom of help')

parser.add_argument('--file-count', type=int, help='number of files to send (default 1000)')      # option that takes a value
parser.add_argument('--file-size', type=int, help='Size of files to send, multiple of 1 MB (default 1)')      # option that takes a value
parser.add_argument('--bandwidth', type=int, help='Max bandwidth, in Mbit/s (default 1000 = 1 Gb/s)')      # option that takes a value
parser.add_argument('--mtu', type=int, help='MTU to use (default 9000)')      # option that takes a value
parser.add_argument('--block-size', type=int, help='Size of data block (default 300000)')      # option that takes a value

args = parser.parse_args()

if args.file_count:
    file_count = args.file_count
if args.file_size:
    file_size = args.file_size
if args.block_size:
    context.block_size = args.block_size
if args.mtu:
    context.mtu = args.mtu
if args.bandwidth:
    bandwidth = args.bandwidth

before_scenario(context, None)
setup_log_config(context, "/dev/shm")
context.bin_dir = "../target/release"

start_throttled_diode(context, (100 * 1000 / 8) * bandwidth)
for i in range(file_count):
    filename = f'test{i}.bin'
    send_file(context, filename, file_size, 1000000)
    try:
        receive_file(context, filename, 20)
        print(f'Got file {i} / {file_count}')
    except Exception as e:
        print(f'Error, lost file {i} / {file_count}')
        break;
    os.unlink(context.files[filename]['path'])
after_scenario(context, None)

