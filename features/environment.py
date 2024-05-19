# functions to be called before or after tests must put here

from tempfile import TemporaryDirectory
import subprocess
import time

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
    context.send_dir = TemporaryDirectory()
    context.send_ratelimit_dir = None
    context.receive_dir = TemporaryDirectory()

    # files metadata
    context.files = {}

    # process instances
    context.proc_diode_receive = None
    context.proc_diode_send = None
    context.proc_network = None
    context.proc_diode_receive_file = None
    context.proc_throttled_fs = None

    # some possible options
    context.network_down_after = None
    context.network_up_after = None
    context.network_max_bandwidth = None
    context.network_drop = None

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
    if context.send_ratelimit_dir:
        context.send_ratelimit_dir.cleanup()
