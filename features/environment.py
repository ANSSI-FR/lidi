# functions to be called before or after tests must be put here

from tempfile import TemporaryDirectory
import subprocess
import time
import os

from features.steps.lidi import stop_throttled_diode
from features.steps.utils import kill_process_safe

# function called before any feature or scenario
def before_all(context):
    # build all applications before running any test
    proc = subprocess.Popen(['just', 'release'])
    proc.communicate()


# function called before each test: initialize context with default values
def before_scenario(context, _feature):
    # test temp dir
    context.base_dir="/dev/shm/lidi"
    
    if not os.path.isdir(context.base_dir):
        os.mkdir(context.base_dir)

    # delete all files in folder (keep directories)
    try:
         files = os.listdir(context.base_dir)
         for file in files:
             file_path = os.path.join(context.base_dir, file)
             if os.path.isfile(file_path):
                 os.remove(file_path)
    except OSError:
        print("Error occurred while deleting files.")

    # Use explicit, static paths for directories
    context.send_dir = os.path.join(context.base_dir, "send")
    context.send_ratelimit_dir = None
    context.receive_dir = os.path.join(context.base_dir, "receive")
    context.log_dir = os.path.join(context.base_dir, "log")
    
    # Clean up directories from previous test
    for directory in [context.send_dir, context.receive_dir, context.log_dir]:
        try:
            if os.path.isdir(directory):
                import shutil
                shutil.rmtree(directory)
        except Exception as e:
            print(f"Error cleaning up directory {directory}: {e}")
    
    # Create directories if they don't exist
    os.makedirs(context.send_dir, exist_ok=True)
    os.makedirs(context.receive_dir, exist_ok=True)
    os.makedirs(context.log_dir, exist_ok=True)

    # files metadata
    context.files = {}

    # process instances
    context.proc_lidi_receive = None
    context.proc_lidi_send = None
    context.proc_lidi_send_file = None
    context.proc_lidi_send_dir = None
    context.proc_network = None
    context.proc_lidi_receive_file = None
    
    # directory containing binaries
    context.bin_dir = "./target/release/"
    
    # some possible options
    context.network_down_after = None
    context.network_up_after = None
    context.network_max_bandwidth = None
    context.bandwidth_must_not_exceed = None
    context.network_drop = None
    context.read_rate = None

    # port configuration
    context.tcp_send_port = 4000
    context.tcp_receive_port = 6000

    context.block_size = None
    context.repair = None
    context.mtu = None

    # display
    context.log_config_lidi_receive = None
    context.log_config_lidi_receive_file = None
    context.log_config_lidi_send = None
    context.log_config_lidi_send_dir = None
    context.log_config_lidi_send_file = None
    context.log_config_network_behavior = None

    context.lidi_config_path = context.base_dir
    
    # setup logging configuration
    setup_log_config(context, context.base_dir)

# function called after every test : cleanup (delete temp directories & kill processes)
def after_scenario(context, _scenario):
    stop_throttled_diode(context)
    
    # first kill processes
    kill_process_safe('proc_lidi_receive', 'lidi-receive', context)
    kill_process_safe('proc_lidi_send', 'lidi-send', context)
    kill_process_safe('proc_lidi_send_file', 'lidi-file-send', context)
    kill_process_safe('proc_lidi_send_dir', 'lidi-dir-send', context)
    kill_process_safe('proc_network', 'lidi-network-simulator', context)
    kill_process_safe('proc_lidi_receive_file', 'lidi-file-receive', context)

    # make sure everything is killed, even throttled_fs (fuse) which uses temp directories
    time.sleep(1)

    # Clear files metadata
    context.files.clear()

def build_log_config(filename, level):
    return f"""
appenders:
  file:
    kind: file
    path: {filename}

root:
  level: {level}
  appenders:
    - file
"""

def setup_log_config(context, log_dir, level="info"):
    context.log_config_lidi_receive = os.path.join(log_dir, "log_config_lidi_receive.yml")
    filename = os.path.join(log_dir, "lidi_receive.log")
    with open(context.log_config_lidi_receive, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_lidi_receive_file = os.path.join(log_dir, "log_config_lidi_receive_file.yml")
    filename = os.path.join(log_dir, "lidi_receive_file.log")
    with open(context.log_config_lidi_receive_file, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_lidi_send = os.path.join(log_dir, "log_config_lidi_send.yml")
    filename = os.path.join(log_dir, "lidi_send.log")
    with open(context.log_config_lidi_send, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_lidi_send_dir = os.path.join(log_dir, "log_config_lidi_send_dir.yml")
    filename = os.path.join(log_dir, "lidi_send_dir.log")
    with open(context.log_config_lidi_send_dir, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_lidi_send_file= os.path.join(log_dir, "log_config_lidi_send_file.yml")
    filename = os.path.join(log_dir, "lidi_send_file.log")
    with open(context.log_config_lidi_send_file, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_network_behavior = os.path.join(log_dir, "log_config_network_behavior.yml")
    filename = os.path.join(log_dir, "network_behavior.log")
    with open(context.log_config_network_behavior, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

