# implementation of steps for diode-file-send and diode-file-receive

from behave import when, then, use_step_matcher
import subprocess
import time
import os
import hashlib

use_step_matcher("cfparse")

def md5sum(filename, blocksize=65536):
    h = hashlib.md5()
    with open(filename, "rb") as f:
        for block in iter(lambda: f.read(blocksize), b""):
            h.update(block)
    return h.hexdigest()

@when('diode-file-send file {name} of size {size}')
def step_impl(context, name, size):

    # extract size & unit
    count = size[0:-2]
    blocksize = size[-2:]

    if blocksize not in ['KB', 'MB', 'GB']:
        raise Exception("Unknown unit")

    # create file
    filename = os.path.join(context.send_dir.name, name)
    proc = subprocess.run(
        f'dd if=/dev/random of={filename} bs={blocksize} count={count}',
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        shell=True,
        timeout=30
    )
    assert proc.returncode == 0

    # store info about the generated file in context
    size = os.stat(filename).st_size
    h = md5sum(filename)

    context.files[name] = { 'size': size, 'hash': h }

    # take care of possible throttled fs to limit tx throughput
    if context.send_ratelimit_dir:
        filename = os.path.join(context.send_ratelimit_dir.name, name)

    # send it (using buffer size of 8192 to limit bursts & packet drops)
    result = subprocess.run(
        f'cargo run --release --bin diode-send-file -- --buffer_size 8192 --to_tcp 127.0.0.1:5000 {filename}',
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        shell=True,
        timeout=60
    )

    print(result.stdout)
    print(result.stderr)
    assert result.returncode == 0

@then('diode-file-receive file {name} in {seconds} seconds')
def step_impl(context, name, seconds):
    # get info about the file
    info = context.files[name]
    size = info['size']
    h = info['hash']

    # where it should be
    filename = os.path.join(context.receive_dir.name, name)

    # wait for it
    seconds = int(seconds)

    for _ in range(seconds):
        time.sleep(1)
        try:
            stat = os.stat(filename)
            if stat.st_size != size:
                # file incomplete, wait for more data
                continue
        except Exception:
            # file not found, wait
            continue

        # file received, check content
        assert md5sum(filename) == h

        # ok => quit
        return

    # loop stops before receiving file
    raise Exception('File not received')

