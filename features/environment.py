# functions to be called before or after tests must put here

from tempfile import TemporaryDirectory
import subprocess
import time
import os

# function call before any feature or scenario
def before_all(context):
    # build all applications before running any test
    proc = subprocess.Popen(['cargo', 'build', '--release', '--bin', 'diode-receive'])
    proc.communicate()

    proc = subprocess.Popen(['cargo', 'build', '--release', '--bin', 'diode-send'])
    proc.communicate()

    proc = subprocess.Popen(['cargo', 'build', '--release', '--bin', 'network-behavior'])
    proc.communicate()

    proc = subprocess.Popen(['cargo', 'build', '--release', '--bin', 'diode-receive-file'])
    proc.communicate()

    proc = subprocess.Popen(['cargo', 'build', '--release', '--bin', 'diode-send-file'])
    proc.communicate()


# function called before every test : initialize context with default values
def before_scenario(context, _feature):
    # test temp dir
    dev_shm="/dev/shm"
    context.send_dir = TemporaryDirectory(dir=dev_shm)
    context.send_ratelimit_dir = None
    context.receive_dir = TemporaryDirectory(dir=dev_shm)
    context.log_dir = TemporaryDirectory(dir=dev_shm)

    # files metadata
    context.files = {}

    # process instances
    context.proc_diode_receive = None
    context.proc_diode_send = None
    context.proc_network = None
    context.proc_diode_receive_file = None
    context.proc_throttled_fs = None

    # some possible optons
    context.network_down_after = None
    context.network_up_after = None
    context.network_max_bandwidth = None
    context.network_drop = None
    context.read_rate = None

    # perf options
    context.mtu = None
    context.block_size = None
    context.send_nb_threads = 1

    # display
    context.quiet = False
    context.log_config_diode_receive = None
    context.log_config_diode_receive_file = None
    context.log_config_diode_send = None

    context.bin_dir = "./target/release/"

    #setup_log_config(context, context.log_dir.name)
    setup_log_config(context, "/dev/shm")


# function called after every test : cleanup (delete temp directories & kill processes)
def after_scenario(context, _feature):
    # first kill processes
    if context.proc_diode_receive:
        context.proc_diode_receive.kill()
    if context.proc_diode_send:
        context.proc_diode_send.kill()
    if context.proc_network:
        context.proc_network.kill()
    if context.proc_diode_receive_file:
        context.proc_diode_receive_file.kill()
    if context.proc_throttled_fs:
        context.proc_throttled_fs.kill()

    # make sure everything is killed, even throttled_fs (fuse) which uses temp directories
    time.sleep(1)

    # delete temp directories
    context.send_dir.cleanup()
    context.receive_dir.cleanup()
    context.log_dir.cleanup()
    if context.send_ratelimit_dir:
        context.send_ratelimit_dir.cleanup()

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

def setup_log_config(context, log_dir, level="trace"):
    context.log_config_diode_receive = os.path.join(log_dir, "log_config_diode_receive.yml")
    filename = os.path.join(log_dir, "diode_receive.log")
    with open(context.log_config_diode_receive, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_diode_send = os.path.join(log_dir, "log_config_diode_send.yml")
    filename = os.path.join(log_dir, "diode_send.log")
    with open(context.log_config_diode_send, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_diode_receive_file = os.path.join(log_dir, "log_config_diode_receive_file.yml")
    filename = os.path.join(log_dir, "diode_receive_file.log")
    with open(context.log_config_diode_receive_file, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

