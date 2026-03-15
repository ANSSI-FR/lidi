
import hashlib
import os
import shutil
import subprocess
from tempfile import TemporaryDirectory
import time


def parse_human_size(size):
    """Parse file size string into bytes."""
    # Extract size & unit
    count = int(size[0:-2])
    unit = size[-2:]

    if unit == 'KB':
        size_in_bytes = count * 1024
    elif unit == 'MB':
        size_in_bytes = count * 1024 * 1024
    elif unit == 'GB':
        size_in_bytes = count * 1024 * 1024 * 1024
    else:
        raise Exception("Unknown unit")
    
    return size_in_bytes

def md5sum(filename, blocksize=65536):
    """Calculate MD5 hash of a file."""
    h = hashlib.md5()
    with open(filename, "rb") as f:
        for block in iter(lambda: f.read(blocksize), b""):
            h.update(block)
    return h.hexdigest()

def create_file(context, filename, size):
    """Create a file with specified size using dd command."""
    file_size = parse_human_size(size)
    count = file_size // 1024
    blocksize = 1024

    proc = subprocess.run(
        f'dd if=/dev/random of={filename} bs={blocksize} count={count}',
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        shell=True,
        timeout=30
    )
    assert proc.returncode == 0

    # Store info in context
    store_file_info(context, filename)

def store_file_info(context, filename):
    """Store file information in the context."""
    # Store info about the generated file in context
    file_size = os.stat(filename).st_size
    file_hash = md5sum(filename)

    name = os.path.basename(filename)

    context.files[name] = { 'size': file_size, 'hash': file_hash, 'path': filename }

def wait_for_file(context, dir, name, seconds, expect_file=True):
    """Wait for a file to be received with content verification."""
    # Get info about the file
    info = context.files[name]
    file_size = info['size']
    file_hash = info['hash']

    # Where it should be
    filename = os.path.join(dir, name)

    # Wait for it
    timeout_seconds = int(seconds)
    timeout_milliseconds = timeout_seconds * 1000
    
    for _ in range(timeout_milliseconds):
        try:
            stat = os.stat(filename)
            if stat.st_size != file_size:
                # File incomplete, wait for more data
                time.sleep(0.001)
                continue
        except (FileNotFoundError, OSError):
            # File not found, wait
            time.sleep(0.001)
            continue

        # File received, check content
        file_content_hash = md5sum(filename)
        if file_content_hash != file_hash:
            raise Exception(f'File content hash mismatch for {name}: expected {file_hash}, got {file_content_hash}')

        if expect_file:
            # OK => delete and quit
            #os.unlink(filename)
            return
        else:
            # OK => delete and raise exception (file should not be received)
            os.unlink(filename)
            raise Exception('File received')

    # Loop stops before receiving file
    raise Exception('File not received')

def test_file(context, dir, name, seconds):
    """Test that a file is received."""
    wait_for_file(context, dir, name, seconds, expect_file=True)

def test_no_file(context, dir, name, seconds):
    """Test that a file is not received."""
    wait_for_file(context, dir, name, seconds, expect_file=False)

def create_and_copy_file(context, name, size):
    """Create a file and copy it to the send directory."""
    temp_dir = TemporaryDirectory(dir=context.base_dir)
    try:
        filename = os.path.join(temp_dir.name, name)
        create_file(context, filename, size)
        shutil.copy(filename, context.send_dir.name)
    finally:
        temp_dir.cleanup()

def create_and_copy_multiple_files(context, files, size):
    """Create multiple files and copy them to the send directory."""
    temp_dir = TemporaryDirectory(dir=context.base_dir)
    try:
        for i in range(int(files)):
            context.counter += 1
            name = str(f"test_file_{context.counter}_{i}")
            filename = os.path.join(temp_dir.name, name)
            create_file(context, filename, size)
            shutil.copy(filename, context.send_dir.name)
    finally:
        temp_dir.cleanup()

def create_and_move_file(context, name, size):
    """Create a file and move it to the send directory."""
    temp_dir = TemporaryDirectory(dir=context.base_dir)
    try:
        filename = os.path.join(temp_dir.name, name)
        create_file(context, filename, size)
        destname = os.path.join(context.send_dir.name, name)
        os.rename(filename, destname)
    finally:
        temp_dir.cleanup()
